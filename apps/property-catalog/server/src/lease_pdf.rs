//! Completion layer over Ontario's unmodified 2229E (2020/12) static PDF.
//! Coordinates below use the 90 DPI rendering of the government's letter-size form.
use crate::{auth::Database, database_error, leasing::LeaseDetails, ApiResult};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
};
use lopdf::{
    content::{Content, Operation},
    dictionary, Dictionary, Document, Object, ObjectId, StringFormat,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

const ORIGINAL: &[u8] = include_bytes!("../../assets/forms/ontario-standard-lease-2229e.pdf");
// Allura is bundled under the SIL Open Font License; see assets/fonts/OFL-Allura.txt.
const SIGNATURE_FONT: &[u8] = include_bytes!("../../assets/fonts/Allura-Regular.ttf");
pub fn validate_supplement(bytes: &[u8]) -> Result<(), String> {
    let pdf = Document::load_mem(bytes).map_err(|_| "Could not read the PDF.".to_string())?;
    if pdf.is_encrypted() || pdf.get_pages().is_empty() {
        return Err("Choose an unencrypted PDF with at least one page.".into());
    }
    if pdf.get_pages().len() > 30 {
        return Err("The additional PDF can contain up to 30 pages.".into());
    }
    Ok(())
}

fn append_supplement(rendered: Vec<u8>, supplement: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let Some(bytes) = supplement else {
        return Ok(rendered);
    };
    validate_supplement(bytes)?;
    let mut lease = Document::load_mem(&rendered).map_err(|e| e.to_string())?;
    let mut extra = Document::load_mem(bytes).map_err(|e| e.to_string())?;
    let lease_pages = lease
        .catalog()
        .map_err(|e| e.to_string())?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|e| e.to_string())?;
    extra.renumber_objects_with(lease.max_id + 1);
    let extra_pages = extra
        .catalog()
        .map_err(|e| e.to_string())?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|e| e.to_string())?;
    let extra_count = extra.get_pages().len() as i64;
    lease.max_id = extra.max_id;
    lease.objects.extend(extra.objects);
    lease
        .get_dictionary_mut(extra_pages)
        .map_err(|e| e.to_string())?
        .set("Parent", lease_pages);
    let pages = lease
        .get_dictionary_mut(lease_pages)
        .map_err(|e| e.to_string())?;
    let count = pages
        .get(b"Count")
        .and_then(Object::as_i64)
        .map_err(|e| e.to_string())?;
    pages
        .get_mut(b"Kids")
        .and_then(Object::as_array_mut)
        .map_err(|e| e.to_string())?
        .push(Object::Reference(extra_pages));
    pages.set("Count", count + extra_count);
    let mut combined = Vec::new();
    lease.save_to(&mut combined).map_err(|e| e.to_string())?;
    Ok(combined)
}
pub async fn original_form() -> (HeaderMap, &'static [u8]) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/pdf".parse().unwrap());
    headers.insert(
        header::CACHE_CONTROL,
        "public, max-age=86400".parse().unwrap(),
    );
    (headers, ORIGINAL)
}

fn text_op_with_font(font: &str, x: f32, y: f32, size: f32, value: &str) -> Vec<Operation> {
    let encoded = encoding_rs::WINDOWS_1252.encode(value).0.into_owned();
    vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![font.into(), size.into()]),
        Operation::new(
            "Tm",
            vec![1.into(), 0.into(), 0.into(), 1.into(), x.into(), y.into()],
        ),
        Operation::new("Tj", vec![Object::String(encoded, StringFormat::Literal)]),
        Operation::new("ET", vec![]),
    ]
}
fn text_op(x: f32, y: f32, size: f32, value: &str) -> Vec<Operation> {
    text_op_with_font("BuildryFont", x, y, size, value)
}
fn put(ops: &mut Vec<Operation>, x: f32, y: f32, value: &str, size: f32) {
    // x and y are measured from the top left of the 765x990 preview image.
    if !value.trim().is_empty() {
        ops.push(Operation::new(
            "rg",
            vec![0.07.into(), 0.32.into(), 0.65.into()],
        ));
        ops.extend(text_op(x * 0.8, (990.0 - y) * 0.8, size + 3.0, value));
    }
}
#[derive(Clone, Copy)]
struct LineAnchor {
    start_x: f32,
    rule_y: f32,
}
impl LineAnchor {
    const fn new(start_x: f32, rule_y: f32) -> Self {
        Self { start_x, rule_y }
    }
}
fn put_on_line(ops: &mut Vec<Operation>, anchor: LineAnchor, value: &str, size: f32) {
    // Anchor to the printed rule, not to a shared row offset. Each field below
    // has its own measured rule start and vertical position in the source PDF.
    put(ops, anchor.start_x + 2.5, anchor.rule_y - 6.5, value, size);
}
fn put_signature_on_line(ops: &mut Vec<Operation>, anchor: LineAnchor, value: &str) {
    if value.trim().is_empty() {
        return;
    }
    let width = ttf_parser::Face::parse(SIGNATURE_FONT, 0)
        .ok()
        .map(|face| {
            value
                .chars()
                .filter_map(|c| {
                    face.glyph_index(c)
                        .and_then(|glyph| face.glyph_hor_advance(glyph))
                })
                .map(u32::from)
                .sum::<u32>() as f32
                / face.units_per_em() as f32
        })
        .unwrap_or(0.0);
    let font_size = if width > 0.0 {
        (210.0 / width).min(19.0).max(12.0)
    } else {
        19.0
    };
    let horizontal_scale = if width > 0.0 {
        (210.0 / (width * font_size)).min(1.0)
    } else {
        1.0
    };
    let encoded = encoding_rs::WINDOWS_1252.encode(value).0.into_owned();
    ops.push(Operation::new(
        "rg",
        vec![0.04.into(), 0.24.into(), 0.55.into()],
    ));
    ops.extend([
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["BuildrySignature".into(), font_size.into()]),
        Operation::new(
            "Tm",
            vec![
                horizontal_scale.into(),
                0.into(),
                0.into(),
                1.into(),
                ((anchor.start_x + 3.0) * 0.8).into(),
                ((990.0 - (anchor.rule_y - 12.0)) * 0.8).into(),
            ],
        ),
        Operation::new("Tj", vec![Object::String(encoded, StringFormat::Literal)]),
        Operation::new("ET", vec![]),
    ]);
}
fn embedded_signature_font(doc: &mut Document) -> Result<ObjectId, String> {
    let face =
        ttf_parser::Face::parse(SIGNATURE_FONT, 0).map_err(|e| format!("Signature font: {e:?}"))?;
    let units = face.units_per_em() as f32;
    let metric = |value: i16| (value as f32 * 1000.0 / units).round() as i64;
    let bbox = face.global_bounding_box();
    let font_file = doc.add_object(lopdf::Stream::new(
        dictionary! {"Length1"=>SIGNATURE_FONT.len() as i64},
        SIGNATURE_FONT.to_vec(),
    ));
    let descriptor=doc.add_object(dictionary!{
        "Type"=>"FontDescriptor","FontName"=>"Allura-Regular","Flags"=>96,
        "FontBBox"=>vec![metric(bbox.x_min).into(),metric(bbox.y_min).into(),metric(bbox.x_max).into(),metric(bbox.y_max).into()],
        "ItalicAngle"=>0,"Ascent"=>metric(face.ascender()),"Descent"=>metric(face.descender()),
        "CapHeight"=>metric(face.capital_height().unwrap_or(face.ascender())),"StemV"=>70,"FontFile2"=>font_file
    });
    let widths: Vec<Object> = (32u8..=255)
        .map(|byte| {
            let encoded = [byte];
            let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(&encoded);
            let advance = decoded
                .chars()
                .next()
                .and_then(|c| face.glyph_index(c))
                .and_then(|glyph| face.glyph_hor_advance(glyph))
                .unwrap_or(0);
            ((advance as f32 * 1000.0 / units).round() as i64).into()
        })
        .collect();
    Ok(doc.add_object(dictionary! {
        "Type"=>"Font","Subtype"=>"TrueType","BaseFont"=>"Allura-Regular",
        "FirstChar"=>32,"LastChar"=>255,"Widths"=>widths,
        "FontDescriptor"=>descriptor,"Encoding"=>"WinAnsiEncoding"
    }))
}
mod line_anchors {
    use super::LineAnchor;
    pub mod p1 {
        use super::LineAnchor;
        pub const LANDLORD: LineAnchor = LineAnchor::new(43.9, 429.7);
        pub const TENANT_LAST: LineAnchor = LineAnchor::new(43.9, 561.5);
        pub const TENANT_FIRST: LineAnchor = LineAnchor::new(449.6, 561.5);
        pub const UNIT: LineAnchor = LineAnchor::new(21.4, 779.3);
        pub const STREET_NUMBER: LineAnchor = LineAnchor::new(269.0, 779.3);
        pub const STREET_NAME: LineAnchor = LineAnchor::new(381.5, 779.3);
        pub const CITY: LineAnchor = LineAnchor::new(21.4, 813.0);
        pub const POSTAL: LineAnchor = LineAnchor::new(629.0, 813.0);
        pub const PARKING: LineAnchor = LineAnchor::new(21.4, 867.0);
    }
    pub mod p2 {
        use super::LineAnchor;
        pub const NOTICE_NUMBER: LineAnchor = LineAnchor::new(133.9, 111.1);
        pub const NOTICE_STREET: LineAnchor = LineAnchor::new(246.5, 111.1);
        pub const NOTICE_CITY: LineAnchor = LineAnchor::new(21.4, 144.9);
        pub const NOTICE_POSTAL: LineAnchor = LineAnchor::new(584.0, 144.9);
        pub const EMAIL: LineAnchor = LineAnchor::new(21.4, 253.3);
        pub const CONTACT: LineAnchor = LineAnchor::new(21.4, 339.9);
        pub const START_DATE: LineAnchor = LineAnchor::new(158.9, 442.2);
        pub const END_DATE: LineAnchor = LineAnchor::new(227.8, 500.7);
        pub const OTHER_TERM: LineAnchor = LineAnchor::new(295.5, 559.8);
        pub const DUE_DAY: LineAnchor = LineAnchor::new(187.4, 670.2);
        pub const BASE_RENT: LineAnchor = LineAnchor::new(449.0, 777.4);
        pub const TOTAL_RENT: LineAnchor = LineAnchor::new(449.5, 927.5);
    }
    pub mod p3 {
        use super::LineAnchor;
        pub const PAYEE: LineAnchor = LineAnchor::new(21.4, 127.9);
        pub const PAYMENT_METHOD: LineAnchor = LineAnchor::new(21.4, 181.9);
        pub const PARTIAL_RENT: LineAnchor = LineAnchor::new(555.0, 264.1);
        pub const NSF_FEE: LineAnchor = LineAnchor::new(195.8, 364.0);
        pub const SERVICES: LineAnchor = LineAnchor::new(21.4, 908.9);
    }
    pub mod p4 {
        use super::LineAnchor;
        pub const UTILITIES: LineAnchor = LineAnchor::new(22.0, 330.4);
        pub const RENT_DISCOUNT: LineAnchor = LineAnchor::new(21.4, 569.3);
        pub const RENT_DEPOSIT: LineAnchor = LineAnchor::new(255.9, 750.7);
    }
    pub mod p5 {
        use super::LineAnchor;
        pub const KEY_DEPOSIT: LineAnchor = LineAnchor::new(315.5, 145.8);
        pub const KEY_DESCRIPTION: LineAnchor = LineAnchor::new(21.4, 286.4);
        pub const SMOKING: LineAnchor = LineAnchor::new(21.4, 615.6);
    }
    pub mod p7 {
        use super::LineAnchor;
        pub const LANDLORD_NAME: LineAnchor = LineAnchor::new(21.4, 213.5);
        pub const LANDLORD_SIGNATURE: LineAnchor = LineAnchor::new(314.0, 213.5);
        pub const LANDLORD_DATE: LineAnchor = LineAnchor::new(584.0, 213.5);
        pub const TENANT_NAME: LineAnchor = LineAnchor::new(21.4, 321.6);
        pub const TENANT_SIGNATURE: LineAnchor = LineAnchor::new(314.0, 321.6);
        pub const TENANT_DATE: LineAnchor = LineAnchor::new(584.0, 321.6);
    }
}
fn check(ops: &mut Vec<Operation>, x: f32, y: f32, yes: bool) {
    if yes {
        // x/y are the measured center of a printed box. Keep the blue mark centered.
        ops.push(Operation::new(
            "rg",
            vec![0.07.into(), 0.32.into(), 0.65.into()],
        ));
        ops.push(Operation::new(
            "re",
            vec![
                ((x - 4.0) * 0.8).into(),
                ((990.0 - (y + 4.0)) * 0.8).into(),
                6.4.into(),
                6.4.into(),
            ],
        ));
        ops.push(Operation::new("f", vec![]));
    }
}
fn check_pair(ops: &mut Vec<Operation>, yes_x: f32, no_x: f32, y: f32, yes: bool) {
    check(ops, yes_x, y, yes);
    check(ops, no_x, y, !yes)
}
fn shorten(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
fn wrap(value: &str, width: usize) -> Vec<String> {
    let mut result = Vec::new();
    for para in value.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            if !line.is_empty() && line.chars().count() + word.chars().count() + 1 > width {
                result.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ')
            }
            line.push_str(word);
        }
        result.push(line);
    }
    result
}
fn wrapped_to_line(
    ops: &mut Vec<Operation>,
    anchor: LineAnchor,
    value: &str,
    width: usize,
    limit: usize,
    size: f32,
) {
    let lines: Vec<_> = wrap(value, width).into_iter().take(limit).collect();
    let first_y = anchor.rule_y - 6.5 - (lines.len().saturating_sub(1) as f32 * 14.0);
    for (i, line) in lines.iter().enumerate() {
        put(
            ops,
            anchor.start_x + 2.5,
            first_y + i as f32 * 14.0,
            line,
            size,
        )
    }
}
fn font_on_page(
    doc: &mut Document,
    page: ObjectId,
    font: ObjectId,
    signature_font: ObjectId,
) -> Result<(), lopdf::Error> {
    let (inline, ids) = doc.get_page_resources(page)?;
    let mut resources = inline.cloned().unwrap_or_default();
    for id in ids.iter().rev() {
        for (key, value) in doc.get_dictionary(*id)?.iter() {
            resources.set(key.clone(), value.clone());
        }
    }
    let mut fonts = match resources.get(b"Font") {
        Ok(Object::Dictionary(d)) => d.clone(),
        Ok(Object::Reference(id)) => doc.get_dictionary(*id)?.clone(),
        _ => Dictionary::new(),
    };
    fonts.set("BuildryFont", font);
    fonts.set("BuildrySignature", signature_font);
    resources.set("Font", fonts);
    doc.get_object_mut(page)?
        .as_dict_mut()?
        .set("Resources", resources);
    Ok(())
}
fn append_page(doc: &mut Document, font: ObjectId, lines: &[String]) -> Result<(), lopdf::Error> {
    let root = doc.catalog()?.get(b"Pages")?.as_reference()?;
    let resources = doc.add_object(dictionary! {"Font"=>dictionary!{"BuildryFont"=>font}});
    let mut ops = Vec::new();
    ops.extend(text_op(36.0, 754.0, 15.0, "Completed lease schedule"));
    ops.extend(text_op(
        36.0,
        737.0,
        8.0,
        "Attached to Ontario Standard Form of Lease 2229E (2020/12).",
    ));
    for (i, line) in lines.iter().enumerate() {
        ops.extend(text_op(36.0, 714.0 - i as f32 * 12.0, 8.0, line));
    }
    let content = doc.add_object(lopdf::Stream::new(
        dictionary! {},
        Content { operations: ops }.encode()?,
    ));
    let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>root,"MediaBox"=>vec![0.into(),0.into(),612.into(),792.into()],"Resources"=>resources,"Contents"=>content});
    let tree = doc.get_object_mut(root)?.as_dict_mut()?;
    let kids = tree.get_mut(b"Kids")?.as_array_mut()?;
    kids.push(page.into());
    let count = tree.get(b"Count")?.as_i64()?;
    tree.set("Count", count + 1);
    Ok(())
}
fn label(lines: &mut Vec<String>, name: &str, value: &str) {
    let text = format!("{name}: {value}");
    lines.extend(wrap(&text, 110));
}
fn address_parts(value: &str) -> (String, String, String, String) {
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    let (number, street) = parts
        .first()
        .and_then(|line| line.split_once(' '))
        .unwrap_or(("", parts.first().copied().unwrap_or("")));
    let city = parts.get(1).copied().unwrap_or("");
    let postal = parts
        .iter()
        .flat_map(|part| part.split_whitespace())
        .rev()
        .take(2)
        .collect::<Vec<_>>();
    let postal = if postal.len() == 2 && postal[0].len() == 3 && postal[1].len() == 3 {
        format!("{} {}", postal[1], postal[0])
    } else {
        String::new()
    };
    (number.into(), street.into(), city.into(), postal)
}
fn render(
    name: &str,
    email: &str,
    details: &LeaseDetails,
    snapshot: &Value,
    landlord_signed_at: Option<&str>,
    signature: Option<&str>,
    signed_at: Option<&str>,
    is_demo: bool,
) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(ORIGINAL).map_err(|e| e.to_string())?;
    if doc.get_pages().len() != 14 {
        return Err("Unexpected Ontario form page count".into());
    }
    let font=doc.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica-Bold","Encoding"=>"WinAnsiEncoding"});
    let signature_font = embedded_signature_font(&mut doc)?;
    let landlord = &snapshot["landlord"];
    let legal = landlord["legal_name"].as_str().unwrap_or("");
    let address = snapshot["address"].as_str().unwrap_or("");
    let room = snapshot["room"].as_str().unwrap_or("");
    let signer = landlord["signer_name"].as_str().unwrap_or("");
    let service = landlord["service_address"].as_str().unwrap_or("");
    let contact = landlord["email"].as_str().unwrap_or("");
    let phone = landlord["phone"].as_str().unwrap_or("");
    let notice_email = if details.email_notices && !details.landlord_notice_email.trim().is_empty()
    {
        details.landlord_notice_email.trim()
    } else {
        contact
    };
    let contact_details =
        if details.email_notices && !details.landlord_contact_details.trim().is_empty() {
            details.landlord_contact_details.trim().to_string()
        } else {
            format!("{notice_email}  {phone}")
        };
    let (street_number, street_name, city, postal) = address_parts(address);
    let (notice_number, notice_street, notice_city, notice_postal) = address_parts(service);
    let (first, last) = name
        .rsplit_once(' ')
        .map(|(a, b)| (a, b))
        .unwrap_or((name, ""));
    let mut pages: Vec<Vec<Operation>> = vec![Vec::new(); 7];
    let p = &mut pages[0];
    put_on_line(p, line_anchors::p1::LANDLORD, &shorten(legal, 85), 8.0);
    put_on_line(p, line_anchors::p1::TENANT_LAST, &shorten(last, 45), 8.0);
    put_on_line(p, line_anchors::p1::TENANT_FIRST, &shorten(first, 35), 8.0);
    put_on_line(p, line_anchors::p1::UNIT, &shorten(room, 35), 8.0);
    put_on_line(p, line_anchors::p1::STREET_NUMBER, &street_number, 8.0);
    put_on_line(
        p,
        line_anchors::p1::STREET_NAME,
        &shorten(&street_name, 55),
        8.0,
    );
    put_on_line(p, line_anchors::p1::CITY, &shorten(&city, 55), 8.0);
    put_on_line(p, line_anchors::p1::POSTAL, &postal, 8.0);
    put_on_line(
        p,
        line_anchors::p1::PARKING,
        &shorten(&details.parking, 95),
        7.0,
    );
    check_pair(
        p,
        32.95,
        95.95,
        904.55,
        details.condo.to_lowercase().starts_with('y'),
    );
    let p = &mut pages[1];
    put_on_line(p, line_anchors::p2::NOTICE_NUMBER, &notice_number, 7.0);
    put_on_line(
        p,
        line_anchors::p2::NOTICE_STREET,
        &shorten(&notice_street, 55),
        7.0,
    );
    put_on_line(p, line_anchors::p2::NOTICE_CITY, &notice_city, 7.0);
    put_on_line(p, line_anchors::p2::NOTICE_POSTAL, &notice_postal, 7.0);
    check_pair(p, 32.95, 95.95, 198.7, details.email_notices);
    put_on_line(p, line_anchors::p2::EMAIL, &shorten(notice_email, 90), 7.0);
    check(p, 32.95, 287.45, true);
    put_on_line(
        p,
        line_anchors::p2::CONTACT,
        &shorten(&contact_details, 110),
        7.0,
    );
    put_on_line(p, line_anchors::p2::START_DATE, &details.start_date, 8.0);
    check(p, 32.95, 490.8, details.term_type == "fixed");
    check(p, 32.95, 523.95, details.term_type == "month-to-month");
    check(p, 32.95, 550.8, details.term_type == "other");
    if details.term_type == "fixed" {
        put_on_line(p, line_anchors::p2::END_DATE, &details.end_date, 8.0)
    }
    if details.term_type == "other" {
        put_on_line(
            p,
            line_anchors::p2::OTHER_TERM,
            &shorten(&details.other_term, 62),
            8.0,
        )
    }
    put_on_line(p, line_anchors::p2::DUE_DAY, &details.rent_due_day, 8.0);
    check(p, 56.6, 687.7, true);
    if !details.monthly_rent.trim().is_empty() {
        put_on_line(
            p,
            line_anchors::p2::BASE_RENT,
            &format!("$ {}", details.monthly_rent),
            8.0,
        );
        put_on_line(
            p,
            line_anchors::p2::TOTAL_RENT,
            &format!("$ {}", details.monthly_rent),
            8.0,
        );
    }
    let p = &mut pages[2];
    put_on_line(p, line_anchors::p3::PAYEE, &shorten(legal, 95), 8.0);
    put_on_line(
        p,
        line_anchors::p3::PAYMENT_METHOD,
        &shorten(&details.rent_payment_method, 115),
        8.0,
    );
    if !details.partial_rent.trim().is_empty() {
        put_on_line(p, line_anchors::p3::PARTIAL_RENT, "See schedule", 7.0)
    }
    if !details.nsf_fee.trim().is_empty() {
        put_on_line(p, line_anchors::p3::NSF_FEE, &details.nsf_fee, 8.0)
    }
    check_pair(p, 434.15, 488.15, 496.75, details.gas_included);
    check_pair(p, 434.15, 488.15, 523.75, details.ac_included);
    check_pair(p, 434.15, 488.15, 550.75, details.storage_included);
    check_pair(p, 434.15, 488.15, 577.7, details.laundry_included);
    check_pair(p, 434.15, 488.15, 604.7, details.guest_parking_included);
    if details.laundry_included {
        check(p, 533.15, 577.7, details.laundry_fee.trim().is_empty());
        check(p, 623.55, 577.7, !details.laundry_fee.trim().is_empty())
    }
    wrapped_to_line(
        p,
        line_anchors::p3::SERVICES,
        &details.services,
        105,
        7,
        7.0,
    );
    let p = &mut pages[3];
    for (y, payer) in [
        (73.3, &details.electricity_payer),
        (100.25, &details.heat_payer),
        (127.3, &details.water_payer),
    ] {
        check(p, 100.45, y, payer == "landlord");
        check(p, 190.75, y, payer == "tenant")
    }
    wrapped_to_line(
        p,
        line_anchors::p4::UTILITIES,
        &details.utilities,
        110,
        8,
        7.0,
    );
    check(p, 32.95, 395.2, details.rent_discount.trim().is_empty());
    check(p, 32.95, 444.7, !details.rent_discount.trim().is_empty());
    wrapped_to_line(
        p,
        line_anchors::p4::RENT_DISCOUNT,
        &details.rent_discount,
        100,
        3,
        7.0,
    );
    if details.rent_deposit.trim().is_empty() || details.rent_deposit.trim() == "0" {
        check(p, 32.95, 692.65, true)
    } else {
        check(p, 32.95, 741.35, true);
        put_on_line(
            p,
            line_anchors::p4::RENT_DEPOSIT,
            &format!("$ {}", details.rent_deposit),
            8.0,
        )
    }
    let p = &mut pages[4];
    if details.key_deposit.trim().is_empty() || details.key_deposit.trim() == "0" {
        check(p, 32.95, 92.35, true)
    } else {
        check(p, 32.95, 136.5, true);
        put_on_line(
            p,
            line_anchors::p5::KEY_DEPOSIT,
            &format!("$ {}", details.key_deposit),
            8.0,
        )
    }
    wrapped_to_line(
        p,
        line_anchors::p5::KEY_DESCRIPTION,
        &details.key_description,
        110,
        3,
        7.0,
    );
    if details.smoking.trim().is_empty() || details.smoking.to_lowercase() == "none" {
        check(p, 32.95, 446.0, true)
    } else {
        check(p, 32.95, 491.0, true);
        wrapped_to_line(p, line_anchors::p5::SMOKING, &details.smoking, 112, 3, 7.0)
    }
    if details.insurance.trim().is_empty() || details.insurance.to_lowercase() == "none" {
        check(p, 32.95, 757.7, true)
    } else {
        check(p, 32.95, 801.9, true)
    }
    let p = &mut pages[5];
    check(p, 32.95, 739.25, details.additional_terms.trim().is_empty());
    check(p, 32.95, 792.0, !details.additional_terms.trim().is_empty());
    let p = &mut pages[6];
    if let Some(date) = landlord_signed_at {
        put_on_line(
            p,
            line_anchors::p7::LANDLORD_NAME,
            &shorten(signer, 42),
            8.0,
        );
        put_signature_on_line(
            p,
            line_anchors::p7::LANDLORD_SIGNATURE,
            snapshot["landlord_signature"].as_str().unwrap_or(signer),
        );
        put_on_line(
            p,
            line_anchors::p7::LANDLORD_DATE,
            date.get(..10).unwrap_or(date),
            8.0,
        );
    }
    put_on_line(p, line_anchors::p7::TENANT_NAME, &shorten(name, 43), 8.0);
    if let Some(signature) = signature {
        put_signature_on_line(p, line_anchors::p7::TENANT_SIGNATURE, signature);
        let date = signed_at.unwrap_or("");
        put_on_line(
            p,
            line_anchors::p7::TENANT_DATE,
            date.get(..10).unwrap_or(date),
            8.0,
        )
    }
    for (i, mut ops) in pages.into_iter().enumerate() {
        let page = *doc
            .get_pages()
            .get(&((i + 1) as u32))
            .ok_or("Missing PDF page")?;
        font_on_page(&mut doc, page, font, signature_font).map_err(|e| e.to_string())?;
        // The government PDF leaves a 0.75 scale and inverted Y transform active.
        // Cancel it before drawing into the page's normal coordinate system.
        ops.insert(
            0,
            Operation::new(
                "cm",
                vec![
                    (4.0_f32 / 3.0).into(),
                    0.into(),
                    0.into(),
                    (-4.0_f32 / 3.0).into(),
                    0.into(),
                    1056.into(),
                ],
            ),
        );
        doc.add_page_contents(
            page,
            Content { operations: ops }
                .encode()
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    if is_demo {
        for page in doc.get_pages().values().copied().collect::<Vec<_>>() {
            font_on_page(&mut doc, page, font, signature_font).map_err(|e| e.to_string())?;
            let banner = Content {
                operations: text_op(
                    25.0,
                    780.0,
                    10.0,
                    "DEMONSTRATION ONLY - NOT A RENTAL AGREEMENT",
                ),
            }
            .encode()
            .map_err(|e| e.to_string())?;
            doc.add_page_contents(page, banner)
                .map_err(|e| e.to_string())?;
        }
    }
    let mut schedule = Vec::new();
    if is_demo {
        label(
            &mut schedule,
            "NOTICE",
            "DEMONSTRATION ONLY - TEST DATA - NOT A RENTAL AGREEMENT",
        );
    }
    label(&mut schedule, "Landlord legal name", legal);
    label(&mut schedule, "Authorized landlord signatory", signer);
    label(&mut schedule, "Landlord notice address", service);
    label(&mut schedule, "Landlord email", notice_email);
    if details.email_notices && !details.landlord_contact_details.trim().is_empty() {
        label(
            &mut schedule,
            "Landlord contact details",
            &details.landlord_contact_details,
        );
    }
    label(&mut schedule, "Landlord phone", phone);
    label(&mut schedule, "Tenant", name);
    label(&mut schedule, "Tenant email", email);
    label(&mut schedule, "Rental address", address);
    label(&mut schedule, "Rental unit", room);
    for (k, v) in [
        ("Unit description", &details.unit_description),
        ("Start date", &details.start_date),
        ("End date", &details.end_date),
        ("Term", &details.term_type),
        ("Other term details", &details.other_term),
        ("Monthly rent", &details.monthly_rent),
        ("Rent due day", &details.rent_due_day),
        ("Payment method", &details.rent_payment_method),
        ("Parking", &details.parking),
        ("Condominium", &details.condo),
        ("Services", &details.services),
        ("Utilities", &details.utilities),
        ("Rent discount", &details.rent_discount),
        ("Partial rent", &details.partial_rent),
        ("NSF fee", &details.nsf_fee),
        ("Rent deposit", &details.rent_deposit),
        ("Key deposit", &details.key_deposit),
        ("Keys and devices", &details.key_description),
        ("Smoking", &details.smoking),
        ("Insurance", &details.insurance),
        ("Additional terms", &details.additional_terms),
        ("Other occupants", &details.occupants),
        ("Notes", &details.notes),
    ] {
        label(&mut schedule, k, v)
    }
    label(
        &mut schedule,
        "Email notices agreed",
        if details.email_notices { "Yes" } else { "No" },
    );
    for (k, v) in [
        ("Gas included", details.gas_included),
        ("Air conditioning included", details.ac_included),
        ("Storage included", details.storage_included),
        ("Laundry included", details.laundry_included),
        ("Guest parking included", details.guest_parking_included),
    ] {
        label(&mut schedule, k, if v { "Yes" } else { "No" })
    }
    label(&mut schedule, "Laundry fee", &details.laundry_fee);
    for (k, v) in [
        ("Electricity paid by", &details.electricity_payer),
        ("Heat paid by", &details.heat_payer),
        ("Water paid by", &details.water_payer),
    ] {
        label(&mut schedule, k, v)
    }
    if let Some(date) = landlord_signed_at {
        label(
            &mut schedule,
            "Landlord electronic signature",
            snapshot["landlord_signature"].as_str().unwrap_or(signer),
        );
        label(&mut schedule, "Landlord signed at", date);
    }
    if let Some(s) = signature {
        label(&mut schedule, "Tenant electronic signature", s);
        label(&mut schedule, "Signed at", signed_at.unwrap_or(""));
    }
    for chunk in schedule.chunks(52) {
        append_page(&mut doc, font, chunk).map_err(|e| e.to_string())?;
    }
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}
async fn document_for(
    db: &Database,
    sql: &str,
    id: &(dyn tokio_postgres::types::ToSql + Sync),
) -> ApiResult<(HeaderMap, Vec<u8>)> {
    let row = db
        .lock()
        .await
        .query_opt(sql, &[id])
        .await
        .map_err(database_error)?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Lease document not found or link expired.".into(),
        ))?;
    let name: String = row.get(0);
    let email: String = row.get(1);
    let details: String = row.get(2);
    let snapshot: String = row.get(3);
    let landlord_signed_at: Option<String> = row.get(4);
    let signature: Option<String> = row.get(5);
    let signed_at: Option<String> = row.get(6);
    let is_demo: bool = row.get(7);
    let additional_pdf: Option<Vec<u8>> = row.get(8);
    let d: LeaseDetails = serde_json::from_str(&details).map_err(database_error)?;
    let s: Value = serde_json::from_str(&snapshot).map_err(database_error)?;
    let bytes = render(
        &name,
        &email,
        &d,
        &s,
        landlord_signed_at.as_deref(),
        signature.as_deref(),
        signed_at.as_deref(),
        is_demo,
    )
    .map_err(database_error)?;
    let bytes = append_supplement(bytes, additional_pdf.as_deref()).map_err(database_error)?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/pdf".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        "inline; filename=ontario-standard-lease.pdf"
            .parse()
            .unwrap(),
    );
    headers.insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    headers.insert("x-document-sha256", hash.parse().map_err(database_error)?);
    Ok((headers, bytes))
}
pub async fn admin_document(
    Path(id): Path<i64>,
    State(db): State<Database>,
) -> ApiResult<(HeaderMap, Vec<u8>)> {
    document_for(&db,"SELECT tenant_name,tenant_email,details::text,landlord_snapshot::text,to_char(landlord_signed_at AT TIME ZONE 'America/Toronto', 'YYYY-MM-DD HH24:MI:SS') || ' America/Toronto',tenant_signature,to_char(signed_at AT TIME ZONE 'America/Toronto', 'YYYY-MM-DD HH24:MI:SS') || ' America/Toronto',is_demo,additional_pdf FROM leases WHERE id=$1",&id).await
}
pub async fn bytes_for_id(db: &Database, id: i64) -> ApiResult<Vec<u8>> {
    let (_,bytes)=document_for(db,"SELECT tenant_name,tenant_email,details::text,landlord_snapshot::text,to_char(landlord_signed_at AT TIME ZONE 'America/Toronto', 'YYYY-MM-DD HH24:MI:SS') || ' America/Toronto',tenant_signature,to_char(signed_at AT TIME ZONE 'America/Toronto', 'YYYY-MM-DD HH24:MI:SS') || ' America/Toronto',is_demo,additional_pdf FROM leases WHERE id=$1",&id).await?;
    Ok(bytes)
}
pub async fn bytes_for_token(db: &Database, hash: &str) -> ApiResult<Vec<u8>> {
    let (_,bytes)=document_for(db,"SELECT tenant_name,tenant_email,details::text,landlord_snapshot::text,to_char(landlord_signed_at AT TIME ZONE 'America/Toronto', 'YYYY-MM-DD HH24:MI:SS') || ' America/Toronto',tenant_signature,to_char(signed_at AT TIME ZONE 'America/Toronto', 'YYYY-MM-DD HH24:MI:SS') || ' America/Toronto',is_demo,additional_pdf FROM leases WHERE token_hash=$1",&hash).await?;
    Ok(bytes)
}
pub async fn guest_document(
    Path(secret): Path<String>,
    State(db): State<Database>,
) -> ApiResult<(HeaderMap, Vec<u8>)> {
    let hash = format!("{:x}", Sha256::digest(secret.as_bytes()));
    document_for(&db,"SELECT tenant_name,tenant_email,details::text,landlord_snapshot::text,to_char(landlord_signed_at AT TIME ZONE 'America/Toronto', 'YYYY-MM-DD HH24:MI:SS') || ' America/Toronto',tenant_signature,to_char(signed_at AT TIME ZONE 'America/Toronto', 'YYYY-MM-DD HH24:MI:SS') || ' America/Toronto',is_demo,additional_pdf FROM leases WHERE token_hash=$1 AND token_expires_at>NOW()",&hash).await
}
pub async fn landlord_document(
    Path(secret): Path<String>,
    State(db): State<Database>,
) -> ApiResult<(HeaderMap, Vec<u8>)> {
    let hash = format!("{:x}", Sha256::digest(secret.as_bytes()));
    document_for(&db,"SELECT tenant_name,tenant_email,details::text,landlord_snapshot::text,to_char(landlord_signed_at AT TIME ZONE 'America/Toronto', 'YYYY-MM-DD HH24:MI:SS') || ' America/Toronto',tenant_signature,to_char(signed_at AT TIME ZONE 'America/Toronto', 'YYYY-MM-DD HH24:MI:SS') || ' America/Toronto',is_demo,additional_pdf FROM leases WHERE landlord_token_hash=$1 AND landlord_token_expires_at>NOW() AND status='signed' AND signed_at IS NOT NULL",&hash).await
}
pub async fn template_document(
    Path(secret): Path<String>,
    State(db): State<Database>,
) -> ApiResult<(HeaderMap, Vec<u8>)> {
    let hash = format!("{:x}", Sha256::digest(secret.as_bytes()));
    let row=db.lock().await.query_opt("SELECT details::text,landlord_snapshot::text,to_char(landlord_signed_at AT TIME ZONE 'America/Toronto', 'YYYY-MM-DD HH24:MI:SS') || ' America/Toronto',additional_pdf FROM lease_templates WHERE landlord_token_hash=$1 AND landlord_token_expires_at>NOW()",&[&hash]).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND,"Template document not found or link expired.".into()))?;
    let details: LeaseDetails =
        serde_json::from_str(&row.get::<_, String>(0)).map_err(database_error)?;
    let snapshot: Value = serde_json::from_str(&row.get::<_, String>(1)).map_err(database_error)?;
    let signed_at: Option<String> = row.get(2);
    let additional_pdf: Option<Vec<u8>> = row.get(3);
    let bytes = render(
        "",
        "",
        &details,
        &snapshot,
        signed_at.as_deref(),
        None,
        None,
        false,
    )
    .map_err(database_error)?;
    let bytes = append_supplement(bytes, additional_pdf.as_deref()).map_err(database_error)?;
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/pdf".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        "inline; filename=ontario-lease-template.pdf"
            .parse()
            .unwrap(),
    );
    headers.insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    Ok((headers, bytes))
}

pub async fn bytes_for_template_id(db: &Database, id: i64) -> ApiResult<Vec<u8>> {
    let row = db.lock().await.query_opt(
        "SELECT details::text,landlord_snapshot::text,additional_pdf FROM lease_templates WHERE id=$1",
        &[&id],
    ).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND,"Template not found.".into()))?;
    let details: LeaseDetails =
        serde_json::from_str(&row.get::<_, String>(0)).map_err(database_error)?;
    let snapshot: Value = serde_json::from_str(&row.get::<_, String>(1)).map_err(database_error)?;
    let additional_pdf: Option<Vec<u8>> = row.get(2);
    let bytes =
        render("", "", &details, &snapshot, None, None, None, false).map_err(database_error)?;
    append_supplement(bytes, additional_pdf.as_deref()).map_err(database_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manually_entered_landlord_notice_details_appear_on_form() {
        let details = LeaseDetails {
            email_notices: true,
            landlord_notice_email: "notices@example.com".into(),
            landlord_contact_details: "Call 555-0132 for lease notices".into(),
            ..Default::default()
        };
        let snapshot =
            serde_json::json!({"landlord":{"email":"profile@example.com","phone":"555-0000"}});
        let bytes = render("", "", &details, &snapshot, None, None, None, false).unwrap();
        let doc = Document::load_mem(&bytes).unwrap();
        let page = *doc.get_pages().get(&2).unwrap();
        let content = String::from_utf8_lossy(&doc.get_page_content(page).unwrap()).to_string();
        assert!(content.contains("notices@example.com"));
        assert!(content.contains("Call 555-0132 for lease notices"));
        assert!(!content.contains("profile@example.com"));
    }
    #[test]
    fn supplementary_pages_follow_the_lease_pages() {
        let original_pages = Document::load_mem(ORIGINAL).unwrap().get_pages().len();
        let combined = append_supplement(ORIGINAL.to_vec(), Some(ORIGINAL)).unwrap();
        let pdf = Document::load_mem(&combined).unwrap();
        assert_eq!(pdf.get_pages().len(), original_pages * 2);
        let last_page = *pdf.get_pages().get(&(original_pages as u32 * 2)).unwrap();
        assert!(!pdf.get_page_content(last_page).unwrap().is_empty());
    }
    #[test]
    fn filled_government_form_keeps_original_pages_and_signature() {
        let details = LeaseDetails {
            start_date: "2026-11-01".into(),
            end_date: "2027-10-31".into(),
            term_type: "fixed".into(),
            monthly_rent: "1250".into(),
            rent_due_day: "1".into(),
            rent_payment_method: "e-transfer".into(),
            parking: "None".into(),
            condo: "No".into(),
            services: "Wi-Fi and laundry included".into(),
            utilities: "Electricity, heat and water paid by landlord".into(),
            rent_deposit: "1250".into(),
            key_deposit: "50".into(),
            partial_rent: "100".into(),
            smoking: "No smoking indoors".into(),
            insurance: "None".into(),
            additional_terms: String::new(),
            unit_description: "Bedroom 1".into(),
            occupants: String::new(),
            notes: String::new(),
            electricity_payer: "landlord".into(),
            heat_payer: "landlord".into(),
            water_payer: "landlord".into(),
            laundry_included: true,
            ..Default::default()
        };
        let snapshot = serde_json::json!({"landlord":{"legal_name":"Buildry Test Inc.","service_address":"123 Main St, Waterloo, ON","email":"landlord@example.com","phone":"555-0000","signer_name":"Alex Owner"},"address":"456 King St, Waterloo, ON","room":"Bedroom 1"});
        let bytes = render(
            "Taylor Tenant",
            "taylor@example.com",
            &details,
            &snapshot,
            Some("2026-10-05"),
            Some("Taylor Tenant"),
            Some("2026-10-06"),
            false,
        )
        .unwrap();
        let doc = Document::load_mem(&bytes).unwrap();
        assert!(doc.get_pages().len() >= 15);
        let page7 = *doc.get_pages().get(&7).unwrap();
        let content = String::from_utf8_lossy(&doc.get_page_content(page7).unwrap()).to_string();
        assert!(content.contains("Taylor Tenant"));
        assert!(content.contains("BuildrySignature"));
        assert!(!content.contains("/s/"));
        std::fs::write("/tmp/buildry-lease-preview.pdf", bytes).unwrap();
    }
}
