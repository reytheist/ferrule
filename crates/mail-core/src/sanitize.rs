use crate::types::{PrivacyReport, RemoteContent, RemoteImagesStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlDocument(String);

impl HtmlDocument {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedBody {
    pub srcdoc: HtmlDocument,
    pub privacy: PrivacyReport,
}

pub fn sanitize_html(raw_html: &str, policy: RemoteContent) -> SanitizedBody {
    let cleaned = ammonia::clean(raw_html);
    let csp = match policy {
        RemoteContent::BlockAll => "default-src 'none'; img-src data:; style-src 'unsafe-inline';",
        RemoteContent::AllowImagesStripTrackers => {
            "default-src 'none'; img-src data: https:; style-src 'unsafe-inline';"
        }
    };
    let doc = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\"><style>body{{margin:0;font-family:Inter,system-ui,sans-serif;font-size:13px;line-height:1.6;color:inherit;}}img{{max-width:100%;height:auto;}}a{{color:#4f86ff;}}</style></head><body>{cleaned}</body></html>"
    );
    SanitizedBody {
        srcdoc: HtmlDocument(doc),
        privacy: PrivacyReport {
            remote_images: RemoteImagesStatus::None,
            trackers: Vec::new(),
        },
    }
}
