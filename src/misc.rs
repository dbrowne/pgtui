use anyhow::Result;
pub fn create_sample_env_file() -> Result<()> {
    use std::fs::File;
    use std::io::Write;

    let content = r#"# Database Controller Configuration
# This TUI can work in two modes:
# 1. Local mode (default): Creates local docker files automatically
# 2. External mode: Uses existing docker setup

# Use local Docker files (true = create/use local files, false = use external)
USE_LOCAL_DOCKER=true

# External Docker configuration (only used if USE_LOCAL_DOCKER=false)
DOCKER_ENV=
DOCKER_COMPOSE_FILE=docker-compose.yml

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
VOLUME_PREFIX=av_

# PgAdmin configuration
PGADMIN_PORT=5050
PGADMIN_DEFAULT_EMAIL=admin@admin.com
PGADMIN_DEFAULT_PASSWORD=admin

# SchemaSpy configuration for database documentation
# Note: ~ will be expanded to your home directory
SCHEMASPY_JAR=~/local/bin/schemaspy-6.2.4.jar
POSTGRES_DRIVER=~/local/bin/postgresql-42.7.7.jar
OUTPUT_DIR=~/db_relations
"#;

    let mut file = File::create(".env.sample")?;
    file.write_all(content.as_bytes())?;

    // Also create a .env if it doesn't exist
    if !std::path::Path::new(".env").exists() {
        let mut env_file = File::create(".env")?;
        env_file.write_all(content.as_bytes())?;
    }

    Ok(())
}
