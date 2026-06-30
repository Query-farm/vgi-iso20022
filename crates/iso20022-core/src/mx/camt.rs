//! Shared camt.053 / camt.054 entry (`Ntry`) model.
//!
//! camt.053 (`BkToCstmrStmt` / `Stmt`) and camt.054 (`BkToCstmrDbtCdtNtfctn` /
//! `Ntfctn`) use the **same** `Ntry` structure, so the entry parser is shared and
//! `camt053_entries` / `camt054_entries` emit identical columns — reconciliation
//! queries are portable across the two.

use super::common::{collect_texts, date_at, money_at};
use super::dom::{self, Node};
use chrono::NaiveDate;
use rust_decimal::Decimal;

/// One `Ntry` (with one `TxDtls` flattened in; multi-`TxDtls` split by `tx_idx`).
#[derive(Debug, Clone, Default)]
pub struct Entry {
    pub entry_idx: i32,
    pub tx_idx: i32,
    pub amount: Option<Decimal>,
    pub ccy: Option<String>,
    pub credit_debit: Option<String>,
    pub reversal: Option<bool>,
    pub status: Option<String>,
    pub booking_date: Option<NaiveDate>,
    pub value_date: Option<NaiveDate>,
    pub account_servicer_ref: Option<String>,
    pub bank_tx_domain: Option<String>,
    pub bank_tx_family: Option<String>,
    pub bank_tx_subfamily: Option<String>,
    pub bank_tx_proprietary: Option<String>,
    pub end_to_end_id: Option<String>,
    pub tx_id: Option<String>,
    pub uetr: Option<String>,
    pub instr_id: Option<String>,
    pub mandate_id: Option<String>,
    pub counterparty_name: Option<String>,
    pub counterparty_iban: Option<String>,
    pub counterparty_agent_bic: Option<String>,
    pub remittance_unstructured: Vec<String>,
    pub remittance_struct_ref: Option<String>,
    pub addtl_entry_info: Option<String>,
}

/// Parse all entries of a camt.053 / camt.054 document. Detects the statement vs
/// notification parent automatically.
pub fn parse_entries(xml: &str) -> Vec<Entry> {
    let Some(root) = dom::parse(xml) else {
        return Vec::new();
    };
    let parent = root
        .find_first("Stmt")
        .or_else(|| root.find_first("Ntfctn"));
    let Some(parent) = parent else {
        return Vec::new();
    };
    entries_of(parent)
}

/// Parse entries from an already-located `Stmt` / `Ntfctn` node.
pub fn entries_of(parent: &Node) -> Vec<Entry> {
    let mut out = Vec::new();
    for (entry_idx, ntry) in parent.children_named("Ntry").enumerate() {
        let amt = money_at(ntry, &["Amt"]);
        let credit_debit = ntry.text_at(&["CdtDbtInd"]);
        let reversal = ntry
            .text_at(&["RvslInd"])
            .map(|v| matches!(v.trim(), "true" | "1"));
        let status = ntry
            .text_at(&["Sts", "Cd"])
            .or_else(|| ntry.text_at(&["Sts"]));
        let booking_date =
            date_at(ntry, &["BookgDt", "Dt"]).or_else(|| date_at(ntry, &["BookgDt", "DtTm"]));
        let value_date =
            date_at(ntry, &["ValDt", "Dt"]).or_else(|| date_at(ntry, &["ValDt", "DtTm"]));
        let btc = ntry.descend(&["BkTxCd"]);
        let addtl_entry_info = ntry.text_at(&["AddtlNtryInf"]);
        let is_credit = credit_debit.as_deref() == Some("CRDT");

        let base = Entry {
            entry_idx: entry_idx as i32,
            tx_idx: 0,
            amount: amt.amount,
            ccy: amt.ccy.clone(),
            credit_debit: credit_debit.clone(),
            reversal,
            status: status.clone(),
            booking_date,
            value_date,
            account_servicer_ref: ntry.text_at(&["AcctSvcrRef"]),
            bank_tx_domain: btc.and_then(|b| b.text_at(&["Domn", "Cd"])),
            bank_tx_family: btc.and_then(|b| b.text_at(&["Domn", "Fmly", "Cd"])),
            bank_tx_subfamily: btc.and_then(|b| b.text_at(&["Domn", "Fmly", "SubFmlyCd"])),
            bank_tx_proprietary: btc.and_then(|b| b.text_at(&["Prtry", "Cd"])),
            addtl_entry_info,
            ..Default::default()
        };

        // Flatten each TxDtls under NtryDtls; multi-TxDtls -> multiple rows.
        let tx_details: Vec<&Node> = ntry
            .children_named("NtryDtls")
            .flat_map(|nd| nd.children_named("TxDtls"))
            .collect();

        if tx_details.is_empty() {
            out.push(base);
        } else {
            for (tx_idx, tx) in tx_details.iter().enumerate() {
                let refs = tx.descend(&["Refs"]);
                let (cp_name, cp_iban, cp_bic) = counterparty(tx, is_credit);
                out.push(Entry {
                    tx_idx: tx_idx as i32,
                    end_to_end_id: refs.and_then(|r| r.text_at(&["EndToEndId"])),
                    tx_id: refs.and_then(|r| r.text_at(&["TxId"])),
                    uetr: refs.and_then(|r| r.text_at(&["UETR"])),
                    instr_id: refs.and_then(|r| r.text_at(&["InstrId"])),
                    mandate_id: refs.and_then(|r| r.text_at(&["MndtId"])),
                    counterparty_name: cp_name,
                    counterparty_iban: cp_iban,
                    counterparty_agent_bic: cp_bic,
                    remittance_unstructured: collect_texts(tx, &["RmtInf"], "Ustrd"),
                    remittance_struct_ref: tx.text_at(&["RmtInf", "Strd", "CdtrRefInf", "Ref"]),
                    ..base.clone()
                });
            }
        }
    }
    out
}

/// Find a `Bal` of the given type code (`OPBD`/`CLBD`/`CLAV`/…) under a
/// `Stmt`/`Ntfctn` and return its signed amount (debit balances negative) and
/// currency. The code is matched on `Bal/Tp/CdOrPrtry/Cd`.
pub fn balance_by_code(parent: &Node, code: &str) -> (Option<Decimal>, Option<String>) {
    for bal in parent.children_named("Bal") {
        if bal.text_at(&["Tp", "CdOrPrtry", "Cd"]).as_deref() == Some(code) {
            let m = money_at(bal, &["Amt"]);
            let signed = match (m.amount, bal.text_at(&["CdtDbtInd"]).as_deref()) {
                (Some(a), Some("DBIT")) => Some(-a),
                (a, _) => a,
            };
            return (signed, m.ccy);
        }
    }
    (None, None)
}

/// The counterparty is the debtor for a credit entry, the creditor for a debit.
fn counterparty(tx: &Node, is_credit: bool) -> (Option<String>, Option<String>, Option<String>) {
    let (party, acct, agt) = if is_credit {
        ("Dbtr", "DbtrAcct", "DbtrAgt")
    } else {
        ("Cdtr", "CdtrAcct", "CdtrAgt")
    };
    let name = tx.text_at(&["RltdPties", party, "Nm"]);
    let iban = tx.text_at(&["RltdPties", acct, "Id", "IBAN"]);
    let bic = tx.text_at(&["RltdAgts", agt, "FinInstnId", "BICFI"]);
    (name, iban, bic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const XML: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.08">
<BkToCstmrStmt><Stmt><Id>STMT-1</Id>
  <Ntry>
    <Amt Ccy="EUR">500.00</Amt><CdtDbtInd>CRDT</CdtDbtInd><Sts><Cd>BOOK</Cd></Sts>
    <BookgDt><Dt>2026-01-02</Dt></BookgDt><ValDt><Dt>2026-01-02</Dt></ValDt>
    <AcctSvcrRef>ASR-1</AcctSvcrRef>
    <BkTxCd><Domn><Cd>PMNT</Cd><Fmly><Cd>RCDT</Cd><SubFmlyCd>ESCT</SubFmlyCd></Fmly></Domn></BkTxCd>
    <NtryDtls><TxDtls>
      <Refs><EndToEndId>E2E-REF-001</EndToEndId><UETR>e3bf1c2a-1111-4aaa-8bbb-1234567890ab</UETR></Refs>
      <RltdPties><Dbtr><Nm>ACME CORP</Nm></Dbtr><DbtrAcct><Id><IBAN>DE89370400440532013000</IBAN></Id></DbtrAcct></RltdPties>
      <RmtInf><Ustrd>INVOICE 998877</Ustrd></RmtInf>
    </TxDtls></NtryDtls>
    <AddtlNtryInf>CREDIT TRANSFER</AddtlNtryInf>
  </Ntry>
</Stmt></BkToCstmrStmt></Document>"#;

    #[test]
    fn parses_credit_entry() {
        let e = parse_entries(XML);
        assert_eq!(e.len(), 1);
        let r = &e[0];
        assert_eq!(r.amount, Some(Decimal::from_str("500.00").unwrap()));
        assert_eq!(r.credit_debit.as_deref(), Some("CRDT"));
        assert_eq!(r.status.as_deref(), Some("BOOK"));
        assert_eq!(r.bank_tx_domain.as_deref(), Some("PMNT"));
        assert_eq!(r.bank_tx_family.as_deref(), Some("RCDT"));
        assert_eq!(r.bank_tx_subfamily.as_deref(), Some("ESCT"));
        assert_eq!(r.end_to_end_id.as_deref(), Some("E2E-REF-001"));
        // CRDT -> counterparty is the debtor.
        assert_eq!(r.counterparty_name.as_deref(), Some("ACME CORP"));
        assert_eq!(
            r.counterparty_iban.as_deref(),
            Some("DE89370400440532013000")
        );
        assert_eq!(r.addtl_entry_info.as_deref(), Some("CREDIT TRANSFER"));
    }
}
