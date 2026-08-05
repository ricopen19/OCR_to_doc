use std::collections::{HashMap, HashSet};
use std::path::Path;

use pdf_inspector::{detect_pdf, extract_pages_markdown, PdfType};

/// GUI に表示するための PDF テキスト分類結果。
pub struct PdfTextClassification {
    pub pdf_type: &'static str,
    pub confidence: f32,
    /// TextBased / Mixed のみ true。埋め込みテキスト活用オプションを提示してよいか。
    pub eligible: bool,
}

/// PDF が埋め込みテキストを持つか（OCR をスキップできる可能性があるか）を高速判定する。
pub fn classify_pdf(path: &Path) -> Result<PdfTextClassification, String> {
    let result = detect_pdf(path).map_err(|e| format!("PDF 判定失敗: {e}"))?;
    let (pdf_type, eligible) = match result.pdf_type {
        PdfType::TextBased => ("TextBased", true),
        PdfType::Mixed => ("Mixed", true),
        PdfType::Scanned => ("Scanned", false),
        PdfType::ImageBased => ("ImageBased", false),
    };
    Ok(PdfTextClassification {
        pdf_type,
        confidence: result.confidence,
        eligible,
    })
}

/// ページ単位の埋め込みテキスト抽出結果。
/// `texts` は 1-indexed ページ番号 -> Markdown。`needs_ocr` はエンコード崩れ等により
/// 抽出結果が信用できず、通常の OCR に回すべきページの集合。
pub struct PageEmbeddedTexts {
    pub texts: HashMap<u32, String>,
    pub needs_ocr: HashSet<u32>,
}

/// PDF 全ページの埋め込みテキストを抽出する。
pub fn extract_page_texts(path: &Path) -> Result<PageEmbeddedTexts, String> {
    let result = extract_pages_markdown(path, None).map_err(|e| format!("テキスト抽出失敗: {e}"))?;

    let mut texts = HashMap::new();
    for page in &result.pages {
        // PageMarkdown.page は 0-indexed。パイプライン内部の 1-indexed ページ番号に合わせる。
        texts.insert(page.page + 1, page.markdown.clone());
    }
    let needs_ocr: HashSet<u32> = result.pages_needing_ocr.into_iter().collect();

    Ok(PageEmbeddedTexts { texts, needs_ocr })
}
