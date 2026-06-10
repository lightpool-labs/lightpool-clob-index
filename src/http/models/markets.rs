use serde::{Deserialize, Serialize};

use crate::domain::Market;

#[derive(Debug, Deserialize)]
pub struct RegisterQuestionRequest {
    pub question: String,
    pub slug: String,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AllocateSlugRequest {
    pub question: String,
}

#[derive(Debug, Serialize)]
pub struct SlugResponse {
    pub slug: String,
}

#[derive(Debug, Serialize)]
pub struct MarketsPageResponse {
    pub markets: Vec<Market>,
    pub total: usize,
    pub limit: u32,
    pub offset: u32,
}
