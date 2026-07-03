pub mod account;
pub mod assets;
pub mod billing;
pub mod r#gen;
pub mod models;
pub mod pat;
pub mod skills;
pub mod status;
pub mod wait;

use crate::output::OutputFormat;
use nolgia_client::Client;

pub struct CommandContext {
    client: Client,
    format: OutputFormat,
}

impl CommandContext {
    pub fn new(client: Client, format: OutputFormat) -> Self {
        Self { client, format }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }
}
