//! `mt103_read(glob)` — one row per MT103 message.

use std::sync::Arc;

use arrow_schema::{Schema, SchemaRef};
use iso20022_core::mt::block;
use iso20022_core::mt::mt103::{self, Mt103};

use super::common::ReadTable;
use super::scan::finish;
use crate::cols::*;

const EXAMPLES: &str = r#"[{"description":"Parse an inline MT103 credit transfer and read its exact settled amount, currency, and parties.","sql":"SELECT senders_ref, value_date, ccy, amount, beneficiary FROM iso20022.main.mt103_read('{1:F01ACMEDEFFAXXX0000000000}{2:I103DEUTDEFFXXXXN}{3:{121:e3bf1c2a-1111-4aaa-8bbb-1234567890ab}}{4:\n:20:TXN-REF-1\n:23B:CRED\n:32A:260101EUR1234,56\n:50K:/DE89370400440532013000\nACME CORP\n:59:/FR1420041010050500013M02606\nWIDGETS SARL\n:70:INVOICE 998877\n:71A:SHA\n:72:/EToE/E2E-REF-001\n-}') WHERE amount > 1000"}]"#;

/// The fixed output schema (column order matches [`build`]).
pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        commented("senders_ref", DataType::Utf8, ":20: sender's reference."),
        commented("bank_op_code", DataType::Utf8, ":23B: bank operation code."),
        commented(
            "instruction_codes",
            list_utf8_type(),
            ":23E: instruction codes (repeatable).",
        ),
        commented("value_date", DataType::Date32, ":32A: value date."),
        commented("ccy", DataType::Utf8, ":32A: currency."),
        commented("amount", money_type(), ":32A: interbank settled amount."),
        commented(
            "instructed_ccy",
            DataType::Utf8,
            ":33B: instructed currency.",
        ),
        commented(
            "instructed_amount",
            money_type(),
            ":33B: instructed amount.",
        ),
        commented("exchange_rate", money_type(), ":36: exchange rate."),
        commented(
            "ordering_customer",
            DataType::Utf8,
            ":50A/F/K: name + address.",
        ),
        commented(
            "ordering_customer_acct",
            DataType::Utf8,
            ":50A/F/K: account / IBAN.",
        ),
        commented("ordering_customer_bic", DataType::Utf8, ":50A: BIC."),
        commented("ordering_institution", DataType::Utf8, ":52A/D:."),
        commented("senders_correspondent", DataType::Utf8, ":53A/B/D:."),
        commented("receivers_correspondent", DataType::Utf8, ":54A/B/D:."),
        commented("third_reimbursement_inst", DataType::Utf8, ":55A/B/D:."),
        commented("intermediary_inst", DataType::Utf8, ":56A/C/D:."),
        commented("account_with_inst", DataType::Utf8, ":57A/B/C/D:."),
        commented(
            "beneficiary",
            DataType::Utf8,
            ":59/59A/59F: name + address.",
        ),
        commented("beneficiary_acct", DataType::Utf8, ":59…: account / IBAN."),
        commented("beneficiary_bic", DataType::Utf8, ":59A: BIC."),
        commented(
            "remittance_info",
            DataType::Utf8,
            ":70: remittance information.",
        ),
        commented("details_of_charges", DataType::Utf8, ":71A: OUR/SHA/BEN."),
        commented(
            "senders_charges",
            list_utf8_type(),
            ":71F: sender's charges (repeatable).",
        ),
        commented(
            "receivers_charges",
            DataType::Utf8,
            ":71G: receiver's charges.",
        ),
        commented("sender_to_receiver_info", DataType::Utf8, ":72:."),
        commented(
            "end_to_end_id",
            DataType::Utf8,
            "Derived end-to-end id (:72: token / UETR).",
        ),
        commented("uetr", DataType::Utf8, "Block-3 {121:} UETR."),
        commented("raw", DataType::Utf8, "The whole message."),
        commented("path", DataType::Utf8, "Source file path."),
    ]))
}

/// Parse one file (one MT103 message) into a batch; emits zero rows if the file
/// has no `:20:` (so an unrelated file under a glob is skipped, not faked).
pub fn build(path: &str, content: &str) -> arrow_array::RecordBatch {
    let rows: Vec<Mt103> = if block::parse(content).has("20") {
        vec![mt103::parse(content)]
    } else {
        Vec::new()
    };
    let s = schema();
    let cols = vec![
        str_col(rows.iter().map(|r| r.senders_ref.clone())),
        str_col(rows.iter().map(|r| r.bank_op_code.clone())),
        list_str_col(rows.iter().map(|r| r.instruction_codes.clone())),
        date_col(rows.iter().map(|r| r.value_date)),
        str_col(rows.iter().map(|r| r.ccy.clone())),
        dec_col(rows.iter().map(|r| r.amount)),
        str_col(rows.iter().map(|r| r.instructed_ccy.clone())),
        dec_col(rows.iter().map(|r| r.instructed_amount)),
        dec_col(rows.iter().map(|r| r.exchange_rate)),
        str_col(rows.iter().map(|r| r.ordering_customer.clone())),
        str_col(rows.iter().map(|r| r.ordering_customer_acct.clone())),
        str_col(rows.iter().map(|r| r.ordering_customer_bic.clone())),
        str_col(rows.iter().map(|r| r.ordering_institution.clone())),
        str_col(rows.iter().map(|r| r.senders_correspondent.clone())),
        str_col(rows.iter().map(|r| r.receivers_correspondent.clone())),
        str_col(rows.iter().map(|r| r.third_reimbursement_inst.clone())),
        str_col(rows.iter().map(|r| r.intermediary_inst.clone())),
        str_col(rows.iter().map(|r| r.account_with_inst.clone())),
        str_col(rows.iter().map(|r| r.beneficiary.clone())),
        str_col(rows.iter().map(|r| r.beneficiary_acct.clone())),
        str_col(rows.iter().map(|r| r.beneficiary_bic.clone())),
        str_col(rows.iter().map(|r| r.remittance_info.clone())),
        str_col(rows.iter().map(|r| r.details_of_charges.clone())),
        list_str_col(rows.iter().map(|r| r.senders_charges.clone())),
        str_col(rows.iter().map(|r| r.receivers_charges.clone())),
        str_col(rows.iter().map(|r| r.sender_to_receiver_info.clone())),
        str_col(rows.iter().map(|r| r.end_to_end_id.clone())),
        str_col(rows.iter().map(|r| r.uetr.clone())),
        str_col(rows.iter().map(|_| Some(content))),
        str_col(rows.iter().map(|_| Some(path))),
    ];
    finish(&s, cols)
}

/// The `mt103_read` table-function descriptor.
pub fn table() -> ReadTable {
    ReadTable {
        name: "mt103_read",
        schema,
        build,
        title: "Read MT103 Credit Transfers",
        doc_llm: "Scan a glob of SWIFT MT103 (single customer credit transfer) text files into one \
                  row per message: ordering customer and beneficiary (name/account/BIC), the exact \
                  :32A: settled amount and currency, instructed amount, charges, remittance info, \
                  the derived end-to-end id, and the block-3 UETR. Use it for payment ingestion and \
                  MT103<->pacs.008 migration-QA joins.",
        doc_md: "Read SWIFT MT103 single customer credit transfers into rows (one per message).",
        keywords: "mt103, swift mt, credit transfer, ordering customer, beneficiary, 32A, 50K, 59, \
                   uetr, end to end id, payments, fin",
        executable_examples: EXAMPLES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_one_message() {
        let m = "{1:F01X}{2:I103X}{4:\n:20:R\n:23B:CRED\n:32A:260101EUR1234,56\n:50K:/123\nACME\n:59:/456\nWIDGETS\n-}";
        let b = build("/x.txt", m);
        assert_eq!(b.num_rows(), 1);
        assert_eq!(b.schema(), schema());
        // A non-MT file is skipped.
        assert_eq!(build("/y.txt", "not a message").num_rows(), 0);
    }
}
