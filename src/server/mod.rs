use rmcp::{ServerHandler, model::ServerInfo};

pub struct T0k3nServer {
    pub root: String,
}

impl T0k3nServer {
    pub fn new(root: String) -> Self {
        Self { root }
    }
}

impl ServerHandler for T0k3nServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "T0K3N-MCP: Token-saving MCP server. \
                 Use read_directory_tree to explore the workspace, \
                 then fetch only the structure before reading content."
                    .into(),
            ),
            ..Default::default()
        }
    }
}
