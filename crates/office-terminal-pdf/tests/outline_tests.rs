use std::sync::Arc;
use office_terminal_pdf::{HayroEngine, PdfEngine};

fn build_pdf_from_objects(objects: &[&str]) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, body).as_bytes());
    }

    let xref_offset = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref_offset
        )
        .as_bytes(),
    );
    out
}

#[test]
fn test_outline_parent_inherits_first_child_destination() {
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 3 0 R >>",
        "<< /Type /Pages /Kids [4 0 R 5 0 R] /Count 2 >>",
        "<< /Type /Outlines /First 6 0 R /Last 6 0 R /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 8 0 R >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 9 0 R >>",
        // Parent has NO /Dest and NO /A:
        "<< /Title (Part I: Foundations) /Parent 3 0 R /First 7 0 R >>",
        // Child has /Dest pointing to second page (5 0 R):
        "<< /Title (Chapter 1: Intro) /Dest [5 0 R /Fit] /Parent 6 0 R >>",
        "<< /Length 0 >>\nstream\nendstream",
        "<< /Length 0 >>\nstream\nendstream",
    ];

    let bytes = Arc::from(build_pdf_from_objects(&objects).into_boxed_slice());
    let engine = HayroEngine::open(bytes).expect("open synthetic PDF");
    let toc = engine.outline();
    assert_eq!(toc.len(), 2);
    assert_eq!(toc[0].title, "Part I: Foundations");
    assert_eq!(
        toc[0].page_index, 1,
        "parent without destination must inherit first child page index"
    );
    assert_eq!(toc[1].title, "Chapter 1: Intro");
    assert_eq!(toc[1].page_index, 1);
}

#[test]
fn test_outline_cycle_detection_prevents_infinite_recursion() {
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 3 0 R >>",
        "<< /Type /Pages /Kids [4 0 R] /Count 1 >>",
        "<< /Type /Outlines /First 5 0 R /Last 5 0 R >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 7 0 R >>",
        // Cyclic outline: item 5 has First pointing to item 6, and item 6 has First pointing back to 5
        "<< /Title (Loop Parent) /Parent 3 0 R /First 6 0 R >>",
        "<< /Title (Loop Child) /Parent 5 0 R /First 5 0 R /Next 6 0 R >>",
        "<< /Length 0 >>\nstream\nendstream",
    ];

    let bytes = Arc::from(build_pdf_from_objects(&objects).into_boxed_slice());
    let engine = HayroEngine::open(bytes).expect("open synthetic PDF");
    // Must terminate cleanly without stack overflow or hanging
    let toc = engine.outline();
    assert_eq!(toc.len(), 2, "cycle must be pruned without looping");
}

#[test]
fn test_name_tree_limits_pruning() {
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /Outlines 3 0 R /Names << /Dests 7 0 R >> >>",
        "<< /Type /Pages /Kids [4 0 R 5 0 R] /Count 2 >>",
        "<< /Type /Outlines /First 6 0 R /Last 6 0 R /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 9 0 R >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 10 0 R >>",
        "<< /Title (Target) /Dest (TgtA) /Parent 3 0 R >>",
        // Name tree intermediate node with Limits
        "<< /Kids [8 0 R] >>",
        "<< /Limits [(TgtA) (TgtZ)] /Names [(TgtA) [5 0 R /Fit]] >>",
        "<< /Length 0 >>\nstream\nendstream",
        "<< /Length 0 >>\nstream\nendstream",
    ];

    let bytes = Arc::from(build_pdf_from_objects(&objects).into_boxed_slice());
    let engine = HayroEngine::open(bytes).expect("open synthetic PDF");
    let toc = engine.outline();
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].title, "Target");
    assert_eq!(
        toc[0].page_index, 1,
        "destination within limits must be resolved"
    );
}
