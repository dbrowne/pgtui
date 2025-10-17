use chrono::{DateTime, Local};

#[derive(Debug, Clone)]
pub struct AppState {
    pub selected_tab: usize,
    pub selected_action: usize,
    pub logs: Vec<LogEntry>,
    pub db_status: DatabaseStatus,
    pub container_status: ContainerStatus,
    pub port_status: PortStatus,
    pub command_output: Vec<String>,
    pub show_popup: Option<PopupType>,
    pub refresh_rate: u64, // seconds
    pub last_refresh: DateTime<Local>,
    pub action_history: Vec<ActionHistory>,
    pub is_loading: bool,
    pub loading_message: String,
}
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub level: LogLevel,
    pub message: String,
}
#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
    Success,
}

#[derive(Debug, Clone)]
pub struct DatabaseStatus {
    pub connected: bool,
    pub connection_count: i32,
    pub database_size: String,
    pub tables_count: i32,
    pub last_check: DateTime<Local>,
    pub version: String,
    pub uptime: String,
}
#[derive(Debug, Clone)]
pub struct ContainerStatus {
    pub postgres_running: bool,
    pub pgadmin_running: bool,
    pub network_exists: bool,
    pub volumes: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct PortStatus {
    pub db_port_available: bool,
    pub pgadmin_port_available: bool,
}

#[derive(Debug, Clone)]
pub enum PopupType {
    Confirm(String, ActionType),
    Error(String),
    Success(String),
    Help,
    Loading(String),
}

#[derive(Debug, Clone)]
pub enum ActionType {
    StartServices,
    StopServices,
    CleanAll,
    DeleteVolumes,
    GenerateDocs,
    ViewDocs,
}

#[derive(Debug, Clone)]
pub struct ActionHistory {
    pub timestamp: DateTime<Local>,
    pub action: String,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub enum ActionProgress {
    Started,
    Update(String),
    Completed(String),
    Failed(String),
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            selected_tab: 0,
            selected_action: 0,
            logs: vec![LogEntry {
                timestamp: Local::now(),
                level: LogLevel::Info,
                message: "Application started".to_string(),
            }],
            db_status: DatabaseStatus {
                connected: false,
                connection_count: 0,
                database_size: "Unknown".to_string(),
                tables_count: 0,
                last_check: Local::now(),
                version: "Unknown".to_string(),
                uptime: "Unknown".to_string(),
            },
            container_status: ContainerStatus {
                postgres_running: false,
                pgadmin_running: false,
                network_exists: false,
                volumes: vec![],
            },
            port_status: PortStatus {
                db_port_available: true,
                pgadmin_port_available: true,
            },
            command_output: vec![],
            show_popup: None,
            refresh_rate: 5,
            last_refresh: Local::now(),
            action_history: vec![],
            is_loading: false,
            loading_message: String::new(),
        }
    }
}
