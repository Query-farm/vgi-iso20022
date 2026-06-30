//! camt.053 — BkToCstmrStmt (`camt053_read` headers + `camt053_entries`).

use super::camt::{balance_by_code, entries_of, Entry};
use super::common::{datetime_at, decimal_at};
use super::dom::{self, Node};
use chrono::{DateTime, FixedOffset};
use rust_decimal::Decimal;

/// One `Stmt` statement header.
#[derive(Debug, Clone, Default)]
pub struct Statement {
    pub msg_id: Option<String>,
    pub creation_dt: Option<DateTime<FixedOffset>>,
    pub stmt_id: Option<String>,
    pub stmt_seq_nb: Option<String>,
    pub account_iban: Option<String>,
    pub account_other: Option<String>,
    pub account_ccy: Option<String>,
    pub account_owner: Option<String>,
    pub from_dt: Option<DateTime<FixedOffset>>,
    pub to_dt: Option<DateTime<FixedOffset>>,
    pub opening_balance: Option<Decimal>,
    pub closing_balance: Option<Decimal>,
    pub closing_available: Option<Decimal>,
    pub ccy: Option<String>,
    pub entry_count: i32,
    pub sum_credits: Option<Decimal>,
    pub sum_debits: Option<Decimal>,
}

/// Parse a camt.053 document into one row per `Stmt`.
pub fn parse(xml: &str) -> Vec<Statement> {
    let Some(root) = dom::parse(xml) else {
        return Vec::new();
    };
    let Some(body) = root.find_first("BkToCstmrStmt") else {
        return Vec::new();
    };
    let grp = body.child("GrpHdr");
    let msg_id = grp.and_then(|g| g.text_at(&["MsgId"]));
    let creation_dt = grp.and_then(|g| datetime_at(g, &["CreDtTm"]));

    body.children_named("Stmt")
        .map(|stmt| statement_of(stmt, msg_id.clone(), creation_dt))
        .collect()
}

/// Build a [`Statement`] from a `Stmt`/`Ntfctn`-shaped node (reused by camt.054).
pub(crate) fn statement_of(
    stmt: &Node,
    msg_id: Option<String>,
    creation_dt: Option<DateTime<FixedOffset>>,
) -> Statement {
    let (opening_balance, op_ccy) = balance_by_code(stmt, "OPBD");
    let (closing_balance, cl_ccy) = balance_by_code(stmt, "CLBD");
    let (closing_available, _) = balance_by_code(stmt, "CLAV");
    let entry_count = stmt.children_named("Ntry").count() as i32;

    Statement {
        msg_id,
        creation_dt,
        stmt_id: stmt.text_at(&["Id"]),
        stmt_seq_nb: stmt
            .text_at(&["ElctrncSeqNb"])
            .or_else(|| stmt.text_at(&["LglSeqNb"])),
        account_iban: stmt.text_at(&["Acct", "Id", "IBAN"]),
        account_other: stmt.text_at(&["Acct", "Id", "Othr", "Id"]),
        account_ccy: stmt.text_at(&["Acct", "Ccy"]),
        account_owner: stmt.text_at(&["Acct", "Ownr", "Nm"]),
        from_dt: datetime_at(stmt, &["FrToDt", "FrDtTm"]),
        to_dt: datetime_at(stmt, &["FrToDt", "ToDtTm"]),
        opening_balance,
        closing_balance,
        closing_available,
        ccy: op_ccy.or(cl_ccy),
        entry_count,
        sum_credits: decimal_at(stmt, &["TxsSummry", "TtlCdtNtries", "Sum"]),
        sum_debits: decimal_at(stmt, &["TxsSummry", "TtlDbtNtries", "Sum"]),
    }
}

/// Per-blob entry point: parse `Ntry` rows from a whole camt.053 document.
pub fn parse_entries(xml: &str) -> Vec<Entry> {
    let Some(root) = dom::parse(xml) else {
        return Vec::new();
    };
    match root.find_first("Stmt") {
        Some(stmt) => entries_of(stmt),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const XML: &str = r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.08">
<BkToCstmrStmt>
  <GrpHdr><MsgId>CAMT053-1</MsgId><CreDtTm>2026-01-03T06:00:00Z</CreDtTm></GrpHdr>
  <Stmt><Id>STMT-1</Id><ElctrncSeqNb>5</ElctrncSeqNb>
    <Acct><Id><IBAN>DE89370400440532013000</IBAN></Id><Ccy>EUR</Ccy><Ownr><Nm>ACME CORP</Nm></Ownr></Acct>
    <FrToDt><FrDtTm>2026-01-01T00:00:00Z</FrDtTm><ToDtTm>2026-01-02T23:59:59Z</ToDtTm></FrToDt>
    <Bal><Tp><CdOrPrtry><Cd>OPBD</Cd></CdOrPrtry></Tp><Amt Ccy="EUR">1000.00</Amt><CdtDbtInd>CRDT</CdtDbtInd></Bal>
    <Bal><Tp><CdOrPrtry><Cd>CLBD</Cd></CdOrPrtry></Tp><Amt Ccy="EUR">1500.00</Amt><CdtDbtInd>CRDT</CdtDbtInd></Bal>
    <TxsSummry><TtlCdtNtries><Sum>500.00</Sum></TtlCdtNtries><TtlDbtNtries><Sum>0.00</Sum></TtlDbtNtries></TxsSummry>
    <Ntry><Amt Ccy="EUR">500.00</Amt><CdtDbtInd>CRDT</CdtDbtInd></Ntry>
  </Stmt>
</BkToCstmrStmt></Document>"#;

    #[test]
    fn parses_statement_header() {
        let s = parse(XML);
        assert_eq!(s.len(), 1);
        let r = &s[0];
        assert_eq!(r.msg_id.as_deref(), Some("CAMT053-1"));
        assert_eq!(r.stmt_id.as_deref(), Some("STMT-1"));
        assert_eq!(r.stmt_seq_nb.as_deref(), Some("5"));
        assert_eq!(r.account_iban.as_deref(), Some("DE89370400440532013000"));
        assert_eq!(
            r.opening_balance,
            Some(Decimal::from_str("1000.00").unwrap())
        );
        assert_eq!(
            r.closing_balance,
            Some(Decimal::from_str("1500.00").unwrap())
        );
        assert_eq!(r.ccy.as_deref(), Some("EUR"));
        assert_eq!(r.entry_count, 1);
        assert_eq!(r.sum_credits, Some(Decimal::from_str("500.00").unwrap()));
    }

    #[test]
    fn parses_entries() {
        let e = parse_entries(XML);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].credit_debit.as_deref(), Some("CRDT"));
    }
}
