use crate::{
    auth::{AuthSession, Database},
    billing::{self, unavailable, Stripe},
    database_error,
    payments::{actor, checkout_status},
    ApiResult,
};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
};
use lopdf::{
    content::{Content, Operation},
    dictionary, Document, Object, ObjectId, Stream, StringFormat,
};
use reqwest::Method;
use std::collections::BTreeMap;

const FONT: &[u8] = include_bytes!("../../assets/fonts/Manrope-Medium.ttf");
#[derive(Clone)]
pub(crate) struct Invoice {
    pub number: String,
    pub issued: String,
    pub due: String,
    pub description: String,
    pub amount: u64,
    pub status: String,
    pub payer_name: String,
    pub payer_email: String,
    pub receiver_name: String,
    pub receiver_email: String,
    pub prepared_by: String,
    pub test_mode: bool,
}
fn display_status(status: &str) -> &'static str {
    match status {
        "paid" => "Paid",
        "recorded" => "Recorded payment",
        "processing" => "Processing",
        "canceled" | "void" => "Canceled",
        "uncollectible" => "Uncollectible",
        _ => "Unpaid",
    }
}
fn money(value: u64) -> String {
    let whole = (value / 100).to_string();
    let mut out = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("${out}.{:02}", value % 100)
}
fn date(value: &str) -> String {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|d| d.format("%b %-d, %Y").to_string())
        .unwrap_or(value.into())
}
struct Canvas<'a> {
    face: ttf_parser::Face<'a>,
    ops: Vec<Operation>,
    characters: BTreeMap<u16, char>,
}
impl<'a> Canvas<'a> {
    fn width(&self, text: &str, size: f32) -> f32 {
        text.chars()
            .map(|c| {
                self.face
                    .glyph_index(c)
                    .and_then(|g| self.face.glyph_hor_advance(g))
                    .unwrap_or(500) as f32
            })
            .sum::<f32>()
            * size
            / self.face.units_per_em() as f32
    }
    fn text(&mut self, text: &str, x: f32, y: f32, size: f32, color: [f32; 3]) {
        let mut encoded = Vec::new();
        for c in text.chars() {
            let glyph = self
                .face
                .glyph_index(c)
                .or_else(|| self.face.glyph_index('?'))
                .unwrap_or(ttf_parser::GlyphId(0));
            self.characters.entry(glyph.0).or_insert(c);
            encoded.extend(glyph.0.to_be_bytes());
        }
        self.ops.extend([
            Operation::new("rg", color.into_iter().map(Object::Real).collect()),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["Manrope".into(), size.into()]),
            Operation::new("Td", vec![x.into(), y.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(encoded, StringFormat::Hexadecimal)],
            ),
            Operation::new("ET", vec![]),
        ]);
    }
    fn right(&mut self, text: &str, right: f32, y: f32, size: f32, color: [f32; 3]) {
        self.text(text, right - self.width(text, size), y, size, color);
    }
    fn wrap(&self, text: &str, width: f32, size: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in text.split_whitespace() {
            let candidate = if line.is_empty() {
                word.into()
            } else {
                format!("{line} {word}")
            };
            if self.width(&candidate, size) > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            if self.width(word, size) > width {
                for c in word.chars() {
                    let next = format!("{line}{c}");
                    if self.width(&next, size) > width && !line.is_empty() {
                        lines.push(std::mem::take(&mut line));
                    }
                    line.push(c);
                }
            } else {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }
    fn block(&mut self, text: &str, x: f32, y: f32, width: f32, size: f32, color: [f32; 3]) -> f32 {
        let lines = self.wrap(text, width, size);
        for (i, line) in lines.iter().enumerate() {
            self.text(line, x, y - i as f32 * (size + 4.0), size, color);
        }
        y - lines.len() as f32 * (size + 4.0)
    }
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 3]) {
        self.ops.extend([
            Operation::new("rg", color.into_iter().map(Object::Real).collect()),
            Operation::new("re", vec![x.into(), y.into(), w.into(), h.into()]),
            Operation::new("f", vec![]),
        ]);
    }
}
fn embedded_font(doc: &mut Document, canvas: &Canvas<'_>) -> ObjectId {
    let face = &canvas.face;
    let units = face.units_per_em() as f32;
    let metric = |v: i16| (v as f32 * 1000.0 / units).round() as i64;
    let bbox = face.global_bounding_box();
    let file = doc.add_object(Stream::new(
        dictionary! {"Length1"=>FONT.len() as i64},
        FONT.to_vec(),
    ));
    let descriptor=doc.add_object(dictionary!{"Type"=>"FontDescriptor","FontName"=>"Manrope-Medium","Flags"=>32,"FontBBox"=>vec![metric(bbox.x_min).into(),metric(bbox.y_min).into(),metric(bbox.x_max).into(),metric(bbox.y_max).into()],"ItalicAngle"=>0,"Ascent"=>metric(face.ascender()),"Descent"=>metric(face.descender()),"CapHeight"=>metric(face.capital_height().unwrap_or(face.ascender())),"StemV"=>80,"FontFile2"=>file});
    let mut widths = Vec::<Object>::new();
    for gid in canvas.characters.keys() {
        widths.push((*gid as i64).into());
        widths.push(
            vec![((face
                .glyph_hor_advance(ttf_parser::GlyphId(*gid))
                .unwrap_or(500) as f32
                * 1000.0
                / units)
                .round() as i64)
                .into()]
            .into(),
        );
    }
    let descendant=doc.add_object(dictionary!{"Type"=>"Font","Subtype"=>"CIDFontType2","BaseFont"=>"Manrope-Medium","CIDSystemInfo"=>dictionary!{"Registry"=>Object::string_literal("Adobe"),"Ordering"=>Object::string_literal("Identity"),"Supplement"=>0},"FontDescriptor"=>descriptor,"CIDToGIDMap"=>"Identity","DW"=>1000,"W"=>widths});
    let mut cmap=String::from("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /BuildryUnicode def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");
    let entries: Vec<_> = canvas.characters.iter().collect();
    for chunk in entries.chunks(100) {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (glyph, c) in chunk {
            let encoded: Vec<u16> = c.to_string().encode_utf16().collect();
            let unicode = encoded
                .iter()
                .map(|u| format!("{u:04X}"))
                .collect::<String>();
            cmap.push_str(&format!("<{glyph:04X}> <{unicode}>\n"));
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let unicode = doc.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));
    doc.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type0","BaseFont"=>"Manrope-Medium","Encoding"=>"Identity-H","DescendantFonts"=>vec![descendant.into()],"ToUnicode"=>unicode})
}
pub(crate) fn render(invoice: &Invoice) -> Result<Vec<u8>, String> {
    let face = ttf_parser::Face::parse(FONT, 0).map_err(|_| "Could not load invoice font.")?;
    let mut canvas = Canvas {
        face,
        ops: Vec::new(),
        characters: BTreeMap::new(),
    };
    let ink = [0.13, 0.25, 0.19];
    let muted = [0.42, 0.49, 0.45];
    let green = [0.19, 0.40, 0.31];
    let line = [0.85, 0.90, 0.87];
    canvas.rect(48., 778., 516., 4., green);
    canvas.text("Buildry", 48., 735., 25., green);
    canvas.right("Invoice", 564., 735., 26., ink);
    canvas.right(&invoice.number, 564., 710., 9., muted);
    if invoice.test_mode {
        canvas.text("TEST MODE", 48., 711., 9., muted);
    }
    let receiver_name = if invoice
        .receiver_name
        .eq_ignore_ascii_case(&invoice.receiver_email)
    {
        ""
    } else {
        invoice.receiver_name.as_str()
    };
    let payer_name = if invoice
        .payer_name
        .eq_ignore_ascii_case(&invoice.payer_email)
    {
        ""
    } else {
        invoice.payer_name.as_str()
    };
    let from_lines = canvas.wrap(receiver_name, 232., 11.).len()
        + canvas.wrap(&invoice.receiver_email, 232., 10.).len();
    let to_lines = canvas.wrap(payer_name, 232., 11.).len()
        + canvas.wrap(&invoice.payer_email, 232., 10.).len();
    let party_bottom = if from_lines.max(to_lines) > 6 {
        canvas.text("FROM", 48., 664., 8., muted);
        let y = canvas.block(receiver_name, 48., 647., 516., 9.5, ink);
        let y = canvas.block(&invoice.receiver_email, 48., y, 516., 9., muted) - 22.;
        canvas.text("BILL TO", 48., y, 8., muted);
        let y = canvas.block(payer_name, 48., y - 17., 516., 9.5, ink);
        canvas.block(&invoice.payer_email, 48., y, 516., 9., muted)
    } else {
        canvas.text("FROM", 48., 664., 8., muted);
        canvas.text("BILL TO", 326., 664., 8., muted);
        let y1 = canvas.block(receiver_name, 48., 647., 232., 11., ink);
        let y1 = canvas.block(&invoice.receiver_email, 48., y1 - 2., 232., 10., muted);
        let y2 = canvas.block(payer_name, 326., 647., 238., 11., ink);
        let y2 = canvas.block(&invoice.payer_email, 326., y2 - 2., 238., 10., muted);
        y1.min(y2)
    };
    let meta = party_bottom.min(570.) - 24.;
    for (x, label, value) in [
        (48., "ISSUED", date(&invoice.issued)),
        (230., "DUE", date(&invoice.due)),
        (420., "STATUS", display_status(&invoice.status).into()),
    ] {
        canvas.text(label, x, meta, 8., muted);
        canvas.text(&value, x, meta - 17., 10., ink);
    }
    let table = meta - 76.;
    canvas.rect(48., table, 516., 30., [0.94, 0.97, 0.95]);
    canvas.text("Description", 60., table + 11., 9., ink);
    canvas.right("Amount (CAD)", 552., table + 11., 9., ink);
    let description_bottom = canvas.block(&invoice.description, 60., table - 26., 364., 11., ink);
    canvas.right(&money(invoice.amount), 552., table - 26., 11., ink);
    let rule = description_bottom - 25.;
    canvas.rect(48., rule, 516., 0.7, line);
    canvas.text("Total (CAD)", 376., rule - 26., 11., ink);
    canvas.right(&money(invoice.amount), 552., rule - 26., 12., ink);
    let due = if matches!(
        invoice.status.as_str(),
        "paid" | "recorded" | "canceled" | "void"
    ) {
        0
    } else {
        invoice.amount
    };
    canvas.text("Amount due", 376., rule - 59., 10., muted);
    canvas.right(&money(due), 552., rule - 59., 19., green);
    canvas.rect(48., 81., 516., 0.7, line);
    canvas.block(
        &format!("Prepared by {}", invoice.prepared_by),
        48.,
        63.,
        516.,
        8.,
        muted,
    );
    canvas.text("CAD", 48., 34., 8., muted);
    canvas.right("1 / 1", 564., 34., 8., muted);
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let font = embedded_font(&mut doc, &canvas);
    let content = doc.add_object(Stream::new(
        dictionary! {},
        Content {
            operations: canvas.ops,
        }
        .encode()
        .map_err(|e| e.to_string())?,
    ));
    let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),612.into(),792.into()],"Resources"=>dictionary!{"Font"=>dictionary!{"Manrope"=>font}},"Contents"=>content});
    doc.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", catalog);
    let info=doc.add_object(dictionary!{"Title"=>Object::string_literal(format!("Invoice {}",invoice.number)),"Creator"=>Object::string_literal("Buildry")});
    doc.trailer.set("Info", info);
    doc.compress();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}
pub async fn download(
    auth: AuthSession,
    State(db): State<Database>,
    Path(id): Path<String>,
) -> ApiResult<(HeaderMap, Vec<u8>)> {
    let user = actor(&auth)?;
    let invoice = if let Some(lease_id) = id
        .strip_prefix("lease-")
        .and_then(|s| s.split_once('-'))
        .and_then(|(lease, _)| lease.parse::<i64>().ok())
    {
        let history =
            billing::payment_history(&db, lease_id, user.id, user.role == "tenant").await?;
        let record = history
            .rows
            .iter()
            .find(|r| format!("lease-{lease_id}-{}", r.id) == id)
            .ok_or((StatusCode::NOT_FOUND, "Invoice not found.".into()))?;
        let row=db.lock().await.query_one("SELECT tenant_name,tenant_email,COALESCE(landlord_snapshot #>> '{landlord,legal_name}','Landlord'),COALESCE(landlord_snapshot #>> '{landlord,email}','') FROM leases WHERE id=$1", &[&lease_id]).await.map_err(database_error)?;
        Invoice {
            number: format!("LEASE-{lease_id}-{}", record.id),
            issued: record.date.clone(),
            due: record.date.clone(),
            description: match record.kind {
                "rent_deposit" => "Last month’s rent deposit",
                "key_deposit" => "Refundable key deposit",
                _ => "Monthly rent",
            }
            .into(),
            amount: record.amount_cents,
            status: record.status.into(),
            payer_name: row.get(0),
            payer_email: row.get(1),
            receiver_name: row.get(2),
            receiver_email: row.get(3),
            prepared_by: "Buildry".into(),
            test_mode: history.test_mode,
        }
    } else {
        uuid::Uuid::parse_str(&id)
            .map_err(|_| (StatusCode::NOT_FOUND, "Invoice not found.".into()))?;
        let stripe = Stripe::configured().ok_or_else(unavailable)?;
        let row=db.lock().await.query_opt("SELECT r.amount_cents,r.description,r.status,r.payer_name,r.payer_email,r.receiver_name,r.receiver_email,(r.created_at AT TIME ZONE 'America/Toronto')::date::text,r.due_date::text,r.bill_number,c.email,r.session_id,r.account_id FROM account_payment_requests r JOIN users c ON c.id=r.creator_id WHERE r.id=$1::text::uuid AND r.test_mode=$2 AND (r.payer_id=$3 OR r.receiver_id=$3 OR r.creator_id=$3 OR (r.payer_id IS NULL AND lower(r.payer_email)=lower($4)) OR (r.receiver_id IS NULL AND lower(r.receiver_email)=lower($4)))", &[&id,&stripe.test_mode(),&user.id,&user.email]).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND,"Invoice not found.".into()))?;
        let mut status: String = row.get(2);
        if status != "canceled" {
            if let Some(session) = row.get::<_, Option<String>>(11) {
                let value = stripe
                    .for_account(row.get(12))
                    .request(
                        Method::GET,
                        &format!("checkout/sessions/{session}?expand[]=payment_intent"),
                        &[],
                        None,
                    )
                    .await?;
                let state = checkout_status(&value).to_string();
                status=db.lock().await.query_one("UPDATE account_payment_requests SET status=CASE WHEN status IN ('paid','canceled') THEN status ELSE $2 END WHERE id=$1::text::uuid RETURNING status", &[&id,&state]).await.map_err(database_error)?.get(0);
            }
        }
        Invoice {
            number: format!("BRY-{:06}", row.get::<_, i64>(9)),
            issued: row.get(7),
            due: row.get(8),
            description: row.get(1),
            amount: row.get::<_, i64>(0) as u64,
            status,
            payer_name: row.get(3),
            payer_email: row.get(4),
            receiver_name: row.get(5),
            receiver_email: row.get(6),
            prepared_by: row.get(10),
            test_mode: stripe.test_mode(),
        }
    };
    let bytes = render(&invoice).map_err(database_error)?;
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/pdf".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    let safe: String = invoice
        .number
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=invoice-{safe}.pdf")
            .parse()
            .unwrap(),
    );
    Ok((headers, bytes))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invoice_pdf_is_real_and_keeps_the_amount_and_parties() {
        let invoice = Invoice {
            number: "BRY-000001".into(),
            issued: "2026-11-06".into(),
            due: "2026-11-20".into(),
            description: "Property maintenance reimbursement".into(),
            amount: 4850,
            status: "requested".into(),
            payer_name: "Alex Martin".into(),
            payer_email: "alex@example.test".into(),
            receiver_name: "Zoé Tremblay".into(),
            receiver_email: "zoe@example.test".into(),
            prepared_by: "zoe@example.test".into(),
            test_mode: true,
        };
        let bytes = render(&invoice).unwrap();
        let doc = Document::load_mem(&bytes).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
        let text = doc.extract_text(&[1]).unwrap();
        assert!(text.contains("48.50"));
        assert!(text.contains("alex@example.test"));
        assert!(text.contains("Zoé"));
        assert!(!text.contains("Paid"));
    }
}
