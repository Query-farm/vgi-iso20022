//! Composed example queries: realistic SQL that combines two or more of the
//! worker's own objects (a reader joined to another reader, a reader feeding an
//! exploder) the way a payments-ops analyst actually uses them.
//!
//! Each example is defined **once** here and referenced from both the schema-level
//! `vgi.example_queries` list (`main.rs`) and the `vgi.example_queries` list of the
//! object that drives the query (its `ReadTable` / `PerMessageTable` descriptor),
//! where the `vgi-lint` agent simulator can see it. Every message is supplied
//! inline (fabricated names / IBANs / BICs), so each query runs offline.

use crate::meta::Example;

/// An MT103 and its pacs.008 twin (same end-to-end id, UETR, and amount).
macro_rules! dual_mt103 {
    () => {
        "{1:F01ACMEDEFFAXXX0000000000}{2:I103DEUTDEFFXXXXN}{3:{121:e3bf1c2a-1111-4aaa-8bbb-1234567890ab}}{4:\n:20:TXN-REF-1\n:23B:CRED\n:32A:260101EUR1234,56\n:50K:/DE89370400440532013000\nACME CORP\n:59:/FR1420041010050500013M02606\nWIDGETS SARL\n:71A:SHA\n:72:/EToE/E2E-REF-001\n-}"
    };
}
macro_rules! dual_pacs008 {
    () => {
        r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>PACS-MSG-1</MsgId></GrpHdr><CdtTrfTxInf><PmtId><EndToEndId>E2E-REF-001</EndToEndId><UETR>e3bf1c2a-1111-4aaa-8bbb-1234567890ab</UETR></PmtId><IntrBkSttlmAmt Ccy="EUR">1234.56</IntrBkSttlmAmt></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#
    };
}

/// A two-payment pacs.008 batch and the pacs.002 status report answering it
/// (the first payment settled, the second rejected AM04).
macro_rules! batch_pacs008 {
    () => {
        r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08"><FIToFICstmrCdtTrf><GrpHdr><MsgId>BATCH-7</MsgId></GrpHdr><CdtTrfTxInf><PmtId><EndToEndId>E2E-A</EndToEndId><UETR>0a1b2c3d-0000-4aaa-8bbb-00000000000a</UETR></PmtId><IntrBkSttlmAmt Ccy="EUR">1500.00</IntrBkSttlmAmt><Cdtr><Nm>WIDGETS SARL</Nm></Cdtr></CdtTrfTxInf><CdtTrfTxInf><PmtId><EndToEndId>E2E-B</EndToEndId><UETR>0a1b2c3d-0000-4aaa-8bbb-00000000000b</UETR></PmtId><IntrBkSttlmAmt Ccy="EUR">98000.00</IntrBkSttlmAmt><Cdtr><Nm>GADGETS GMBH</Nm></Cdtr></CdtTrfTxInf></FIToFICstmrCdtTrf></Document>"#
    };
}
macro_rules! batch_pacs002 {
    () => {
        r#"<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.002.001.10"><FIToFIPmtStsRpt><GrpHdr><MsgId>STS-7</MsgId></GrpHdr><OrgnlGrpInfAndSts><OrgnlMsgId>BATCH-7</OrgnlMsgId></OrgnlGrpInfAndSts><TxInfAndSts><OrgnlUETR>0a1b2c3d-0000-4aaa-8bbb-00000000000a</OrgnlUETR><TxSts>ACSC</TxSts></TxInfAndSts><TxInfAndSts><OrgnlUETR>0a1b2c3d-0000-4aaa-8bbb-00000000000b</OrgnlUETR><TxSts>RJCT</TxSts><StsRsnInf><Rsn><Cd>AM04</Cd></Rsn></StsRsnInf></TxInfAndSts></FIToFIPmtStsRpt></Document>"#
    };
}

/// Two consecutive MT940 statements; STMT-2's reported closing balance is 50.50
/// higher than its opening balance plus its lines.
macro_rules! two_mt940 {
    () => {
        "{1:F01X}{2:O940X}{4:\n:20:STMT-1\n:25:DE89370400440532013000\n:28C:1/1\n:60F:C260101EUR1000,00\n:61:2601020102C500,00NTRFNONREF//BANK-A\n:61:2601030103D250,50NMSCCUST-REF//BANK-B\n:62F:C260103EUR1249,50\n-}{1:F01X}{2:O940X}{4:\n:20:STMT-2\n:25:DE89370400440532013000\n:28C:2/1\n:60F:C260103EUR1249,50\n:61:2601040104D100,00NMSCREF-2//BANK-C\n:62F:C260104EUR1200,00\n-}"
    };
}

/// MT103 <-> pacs.008 dual-running migration QA (`mt103_read` JOIN `pacs008_read`).
pub const MT103_PACS008_EQUIVALENCE: Example = Example {
    description: "Migration QA: pair an MT103 with its pacs.008 twin on end_to_end_id and check \
                  that the settled amount, currency, and UETR agree in both formats.",
    sql: concat!(
        "SELECT m.end_to_end_id, m.ccy, m.amount AS mt_amount, p.amount AS mx_amount, \
         m.amount = p.amount AND m.ccy = p.ccy AS amount_match, m.uetr = p.uetr AS uetr_match \
         FROM iso20022.main.mt103_read('",
        dual_mt103!(),
        "') AS m JOIN iso20022.main.pacs008_read('",
        dual_pacs008!(),
        "') AS p USING (end_to_end_id) ORDER BY m.end_to_end_id"
    ),
};

/// Payment-status reconciliation (`pacs008_read` LEFT JOIN `pacs002_read`).
pub const PACS008_STATUS: Example = Example {
    description: "Status reconciliation: join each pacs.008 credit transfer to the pacs.002 \
                  status report on UETR to see which payments settled (ACSC) and which were \
                  rejected, with the reason code.",
    sql: concat!(
        "SELECT p.end_to_end_id, p.creditor_name, p.amount, p.ccy, s.tx_status, \
         s.status_reason_code FROM iso20022.main.pacs008_read('",
        batch_pacs008!(),
        "') AS p LEFT JOIN iso20022.main.pacs002_read('",
        batch_pacs002!(),
        "') AS s ON s.orig_uetr = p.uetr ORDER BY p.end_to_end_id"
    ),
};

/// MT940 balance roll-forward (`mt940_read` feeding `mt940_lines`).
pub const MT940_ROLL_FORWARD: Example = Example {
    description: "Statement roll-forward check: explode each MT940 statement's :61: lines, net \
                  credits against debits, and compare opening balance + net movement with the \
                  reported closing balance (here STMT-2 is off by 50.50).",
    sql: concat!(
        "SELECT transaction_ref, opening_balance, \
         sum(CASE WHEN credit_debit IN ('D', 'RC') THEN -amount ELSE amount END) AS net_movement, \
         closing_balance, opening_balance + net_movement = closing_balance AS balanced \
         FROM iso20022.main.mt940_lines((SELECT transaction_ref, opening_balance, closing_balance, \
         raw FROM iso20022.main.mt940_read('",
        two_mt940!(),
        "'))) GROUP BY transaction_ref, opening_balance, closing_balance ORDER BY transaction_ref"
    ),
};

/// Every composed example, in the order the schema-level list shows them.
pub const ALL: &[Example] = &[
    MT103_PACS008_EQUIVALENCE,
    PACS008_STATUS,
    MT940_ROLL_FORWARD,
];
