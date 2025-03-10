use std::collections::HashMap;

#[derive(serde::Deserialize, Debug)]
pub struct JumpTableRead {
    pub map: HashMap<u64, u64>,
}
