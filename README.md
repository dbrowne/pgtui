# PostgreSQL/TimescaleDB TUI Controller

A terminal user interface (TUI) application for managing the PostgreSQL/TimescaleDB Docker container for the alphavantage [secmaster](https://github.com/dbrowne/alphavantage) written in Rust using Ratatui.

![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)
![PostgreSQL](https://img.shields.io/badge/postgres-%23316192.svg?style=for-the-badge&logo=postgresql&logoColor=white)
![Docker](https://img.shields.io/badge/docker-%230db7ed.svg?style=for-the-badge&logo=docker&logoColor=white)

## Features

- 🚀 **Container Management**: Start, stop, and manage PostgreSQL/TimescaleDB containers
- 📊 **Real-time Monitoring**: View database status, connections, and container health
- 📚 **Documentation Generation**: Automatically generate database schema documentation using SchemaSpy
- 🔄 **Auto-refresh**: Status updates every 5 seconds
- 🎨 **Interactive TUI**: Navigate with keyboard shortcuts through multiple tabs
- 🐳 **Docker Integration**: Seamless Docker and Docker Compose management
- 💾 **Persistent Storage**: Manage Docker volumes for data persistence
- 🔍 **Log Viewing**: Built-in application logs and Docker container logs

## Prerequisites

- Rust (latest stable version)
- Docker and Docker Compose
- Java (for SchemaSpy documentation generation)
- Linux/Unix environment (uses Unix-specific features)

### Optional for Documentation Generation
- SchemaSpy JAR file (6.2.4 or later)
- PostgreSQL JDBC driver

## Installation

1. Clone the repository:
```bash
git clone <repository-url>
cd pgtui
```

2. Build the application:
```bash
cargo build --release
```

3. Set up environment variables by copying the sample:
```bash
cp .env.sample .env
```

4. Edit `.env` to configure your settings (see Configuration section)

5. Run the application:
```bash
cargo run
```

## Configuration

The application supports two modes of operation:

### 1. Local Mode (Default)
The TUI automatically creates and manages local Docker configuration files:
- `docker-compose.yml`
- `.env.docker`
- `docker-env.sh`

### 2. External Mode
Use your existing Docker setup by setting `USE_LOCAL_DOCKER=false` in `.env`

### Environment Variables (.env)

```bash
# Use local Docker files (true = create/use local files, false = use external)
USE_LOCAL_DOCKER=true

# Database connection settings
DB_NAME=sec_master
DB_HOST=localhost
DB_PORT=6433
DB_USER=ts_user
DB_PASS=dev_pw

# Container configuration
CONTAINER_NAME=ts_pg_av_dev
NETWORK_NAME=av_network_dev
COMPOSE_SERVICE=av_timescaledb

# PgAdmin configuration
PGADMIN_PORT=5050
PGADMIN_DEFAULT_EMAIL=admin@admin.com
PGADMIN_DEFAULT_PASSWORD=admin

# SchemaSpy configuration (for documentation generation)
SCHEMASPY_JAR=~/local/bin/schemaspy-6.2.4.jar
POSTGRES_DRIVER=~/local/bin/postgresql-42.7.7.jar
OUTPUT_DIR=~/db_relations
```

## Usage

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Tab` | Switch between tabs (forward) |
| `Shift+Tab` | Switch between tabs (backward) |
| `↑/↓` | Navigate through actions |
| `Enter` | Execute selected action |
| `r` | Refresh status |
| `q` | Quit application |
| `?` | Show help |
| `y/n` | Confirm/cancel in dialogs |

### Tabs

1. **Status Tab**: View real-time database and container status
    - Database connection status
    - Container health (PostgreSQL, PgAdmin)
    - Port availability
    - Network status
    - Volume information

2. **Actions Tab**: Execute container and database operations
    - Start Services
    - Stop Services
    - Clean All (remove containers and volumes)
    - Delete Volumes
    - Generate Documentation
    - View Documentation
    - Open PSQL terminal
    - View Docker logs

3. **Logs Tab**: Application event logs
    - Info, Warning, Error, and Success messages
    - Timestamped entries
    - Color-coded by severity

4. **History Tab**: Track executed actions
    - Command history
    - Success/failure status
    - Execution timestamps

## Features in Detail

### Database Documentation Generation
The TUI integrates with SchemaSpy to generate comprehensive HTML documentation of your database schema:
- Entity Relationship Diagrams
- Table relationships
- Column details and constraints
- Database statistics

To use this feature:
1. Download [SchemaSpy](http://schemaspy.org/)
2. Download [PostgreSQL JDBC Driver](https://jdbc.postgresql.org/download/)
3. Configure paths in `.env`
4. Select "Generate Docs" from the Actions tab

### Container Management
The application manages a complete PostgreSQL/TimescaleDB stack:
- **TimescaleDB**: Time-series optimized PostgreSQL
- **PgAdmin**: Web-based database administration
- **Docker Networks**: Isolated container networking
- **Persistent Volumes**: Data persistence across container restarts

### Real-time Monitoring
- Automatic status refresh every 5 seconds
- Database metrics (size, connections, uptime)
- Container health checks
- Port availability monitoring
- Network status validation

## Docker Services

The default Docker Compose configuration includes:

```yaml
services:
  av_timescaledb:
    - TimescaleDB (PostgreSQL 15 with TimescaleDB extension)
    - Port: 6433 (configurable)
    - Persistent data volume
    
  pgadmin:
    - PgAdmin 4 web interface
    - Port: 5050 (configurable)
    - Persistent configuration volume
```

## Architecture

### Project Structure
```
pgtui/
├── src/
│   ├── main.rs        # Application entry point
│   ├── app.rs         # Core application logic
│   ├── runner.rs      # Event loop and input handling
│   ├── setup.rs       # Configuration management
│   ├── structs.rs     # Data structures
│   ├── ui.rs          # Main UI rendering
│   ├── tabs/          # Tab components
│   │   ├── status.rs  # Status display
│   │   ├── actions.rs # Actions menu
│   │   ├── logs.rs    # Log viewer
│   │   └── history.rs # Action history
│   └── misc.rs        # Utility functions
├── docker-compose.yml # Docker services definition
├── .env              # Configuration file
└── Cargo.toml        # Rust dependencies
```

### Key Dependencies
- **ratatui**: Terminal UI framework
- **crossterm**: Cross-platform terminal manipulation
- **tokio**: Async runtime
- **sqlx**: PostgreSQL database driver
- **chrono**: Date and time handling
- **anyhow**: Error handling

## Troubleshooting

### Database Connection Issues
- Ensure Docker containers are running
- Check port availability (default: 6433)
- Verify credentials in `.env`

### Documentation Generation Fails
- Verify Java is installed: `java -version`
- Check SchemaSpy JAR path exists
- Ensure PostgreSQL JDBC driver is available
- Database must be accessible with configured credentials

### Container Won't Start
- Check port conflicts: `lsof -i :6433`
- Verify Docker daemon is running: `docker info`
- Clean up old containers: use "Clean All" action

### Permission Errors
- Ensure user is in docker group: `sudo usermod -aG docker $USER`
- Check file permissions on docker-env.sh: `chmod +x docker-env.sh`

## Development

### Building from Source
```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Format code
cargo fmt

# Run linter
cargo clippy
```

### Contributing
1. Fork the repository
2. Create a feature branch
3. Commit your changes
4. Push to the branch
5. Create a Pull Request

## License

[Specify your license here]

## Acknowledgments

- [Ratatui](https://github.com/ratatui-org/ratatui) - TUI framework
- [TimescaleDB](https://www.timescale.com/) - Time-series PostgreSQL extension
- [SchemaSpy](http://schemaspy.org/) - Database documentation tool
- [Docker](https://www.docker.com/) - Container platform

## Support

For issues, questions, or suggestions, please open an issue on the GitHub repository.