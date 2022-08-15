use crate::adapter::Adapter;
use crate::Result;

pub struct Session {
    _private: (),
}

impl Session {
    pub async fn new() -> Result<Self> {
        Ok(Session { _private: () })
    }

    pub async fn default_adapter(&self) -> Option<Adapter> {
        Some(Adapter::new())
    }
}
