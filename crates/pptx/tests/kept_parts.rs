//! Package parts the model doesn't represent survive an import and export.

use std::io::{Cursor, Read, Write};

use deckcraft_model::defaults;
use deckcraft_pptx::{export, import};

const ITEM: &[u8] = b"<?xml version=\"1.0\"?><root xmlns=\"urn:example:data\"><v>42</v></root>";
const PROPS: &[u8] = b"<?xml version=\"1.0\"?><ds:datastoreItem xmlns:ds=\"http://schemas.openxmlformats.org/officeDocument/2006/customXml\" ds:itemID=\"{11111111-2222-3333-4444-555555555555}\"/>";
const CUSTOM: &[u8] = b"<?xml version=\"1.0\"?><Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/custom-properties\"><property fmtid=\"{D5CDD505-2E9C-101B-9397-08002B2CF9AE}\" pid=\"2\" name=\"Owner\"><lpwstr xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\">team</lpwstr></property></Properties>";

fn read_zip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).expect("zip");
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).expect("entry");
            let mut d = vec![];
            f.read_to_end(&mut d).expect("read");
            (f.name().to_string(), d)
        })
        .collect()
}

fn text(files: &[(String, Vec<u8>)], name: &str) -> String {
    files.iter().find(|(n, _)| n == name).map(|(_, d)| String::from_utf8_lossy(d).into_owned()).unwrap_or_default()
}

/// Add the custom parts to a package written by the exporter.
fn with_custom_parts(base: &[u8]) -> Vec<u8> {
    let mut files = read_zip(base);
    for (name, data) in &mut files {
        let add = |data: &mut Vec<u8>, rel: &str| {
            let s = String::from_utf8_lossy(data).replace("</Relationships>", &format!("{rel}</Relationships>"));
            *data = s.into_bytes();
        };
        match name.as_str() {
            "[Content_Types].xml" => {
                let s = String::from_utf8_lossy(data).replace(
                    "</Types>",
                    "<Override PartName=\"/customXml/item1.xml\" ContentType=\"application/xml\"/>\\
                     <Override PartName=\"/customXml/itemProps1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.customXmlProperties+xml\"/>\\
                     <Override PartName=\"/docProps/custom.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.custom-properties+xml\"/></Types>",
                );
                *data = s.into_bytes();
            }
            "_rels/.rels" => add(
                data,
                "<Relationship Id=\"rIdC\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties\" Target=\"docProps/custom.xml\"/>",
            ),
            "ppt/_rels/presentation.xml.rels" => add(
                data,
                "<Relationship Id=\"rIdX\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml\" Target=\"../customXml/item1.xml\"/>",
            ),
            _ => {}
        }
    }
    files.push(("customXml/item1.xml".into(), ITEM.to_vec()));
    files.push(("customXml/itemProps1.xml".into(), PROPS.to_vec()));
    files.push((
        "customXml/_rels/item1.xml.rels".into(),
        b"<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps\" Target=\"itemProps1.xml\"/></Relationships>".to_vec(),
    ));
    files.push(("docProps/custom.xml".into(), CUSTOM.to_vec()));
    let mut zw = zip::ZipWriter::new(Cursor::new(vec![]));
    for (n, d) in files {
        zw.start_file(n, zip::write::SimpleFileOptions::default()).expect("start");
        zw.write_all(&d).expect("write");
    }
    zw.finish().expect("finish").into_inner()
}

#[test]
fn custom_xml_and_custom_properties_survive_a_save() {
    let base = export(&defaults::new_presentation(None)).expect("export");
    let src = with_custom_parts(&base);

    let deck = import(&src).expect("import");
    let out = read_zip(&export(&deck).expect("export again"));

    assert_eq!(out.iter().find(|(n, _)| n == "customXml/item1.xml").map(|(_, d)| d.as_slice()), Some(ITEM));
    assert_eq!(out.iter().find(|(n, _)| n == "customXml/itemProps1.xml").map(|(_, d)| d.as_slice()), Some(PROPS));
    assert_eq!(out.iter().find(|(n, _)| n == "docProps/custom.xml").map(|(_, d)| d.as_slice()), Some(CUSTOM));
    assert!(text(&out, "_rels/.rels").contains("docProps/custom.xml"));
    assert!(text(&out, "ppt/_rels/presentation.xml.rels").contains("../customXml/item1.xml"));
    assert!(text(&out, "customXml/_rels/item1.xml.rels").contains("itemProps1.xml"));
    assert!(text(&out, "[Content_Types].xml").contains("/docProps/custom.xml"));

    // A second round trip is stable.
    let again = read_zip(&export(&import(&export(&deck).expect("e")).expect("i")).expect("e2"));
    assert_eq!(again.iter().filter(|(n, _)| n.starts_with("customXml/") && !n.contains("_rels")).count(), 2);
}
