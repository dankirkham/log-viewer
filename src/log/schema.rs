use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename = "msq")]
pub struct Tune {
    #[serde(rename = "page", default)]
    pub pages: Vec<Page>,
}

#[derive(Debug, Deserialize)]
pub struct Page {
    #[serde(rename = "constant", default)]
    pub constants: Vec<Constant>,
}

#[derive(Debug, Deserialize)]
pub struct Constant {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@units", default)]
    pub units: Option<String>,
    #[serde(rename = "@rows", default)]
    pub rows: Option<usize>,
    #[serde(rename = "@cols", default)]
    pub cols: Option<usize>,
    #[serde(rename = "$text", default)]
    pub value: String,
}
