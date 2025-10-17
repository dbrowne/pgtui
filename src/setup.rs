use dotenv::dotenv;
use std::env;
// Configuration structure to hold all settings
#[derive(Debug, Clone)]
pub struct Config {
    pub docker_env: String,
    pub docker_compose_file: String,
    pub docker_compose_command: String,
    pub schemaspy_jar: String,
    pub postgres_driver: String,
    pub db_name: String,
    pub db_host: String,
    pub db_user: String,
    pub db_port: u16,
    pub db_pass: String,
    pub output_dir: String,
    pub container_name: String,
    pub pgadmin_port: u16,
    pub network_name: String,
    pub compose_service: String,
    pub volume_prefix: String,
}
impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        // Load .env file if it exists
        dotenv().ok();

        // Expand ~ in paths
        let expand_tilde = |path: &str| -> String {
            if path.starts_with("~/") {
                format!("{}{}", env::var("HOME").unwrap_or_default(), &path[1..])
            } else {
                path.to_string()
            }
        };

        // Check if we should use local docker files
        let use_local_docker = env::var("USE_LOCAL_DOCKER")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .unwrap_or(true);

        // If using local docker files, ensure they exist
        if use_local_docker {
            Self::ensure_docker_files()?;
        }

        // Build docker-compose command based on configuration
        let docker_compose_command = if use_local_docker {
            // Use local files
            "./docker-env.sh docker-compose -f docker-compose.yml".to_string()
        } else if let Ok(docker_env) = env::var("DOCKER_ENV") {
            if docker_env.is_empty() {
                "docker-compose".to_string()
            } else {
                format!("{} docker-compose", docker_env)
            }
        } else {
            "docker-compose".to_string()
        };

        Ok(Self {
            docker_env: env::var("DOCKER_ENV").unwrap_or_else(|_| String::new()),
            docker_compose_file: env::var("DOCKER_COMPOSE_FILE")
                .unwrap_or_else(|_| "docker-compose.yml".to_string()),
            docker_compose_command,
            schemaspy_jar: expand_tilde(
                &env::var("SCHEMASPY_JAR")
                    .unwrap_or_else(|_| "~/local/bin/schemaspy-6.2.4.jar".to_string()),
            ),
            postgres_driver: expand_tilde(
                &env::var("POSTGRES_DRIVER")
                    .unwrap_or_else(|_| "~/local/bin/postgresql-42.7.7.jar".to_string()),
            ),
            db_name: env::var("DB_NAME").unwrap_or_else(|_| "sec_master".to_string()),
            db_host: env::var("DB_HOST").unwrap_or_else(|_| "localhost".to_string()),
            db_user: env::var("DB_USER").unwrap_or_else(|_| "ts_user".to_string()),
            db_port: env::var("DB_PORT")
                .unwrap_or_else(|_| "6433".to_string())
                .parse()
                .unwrap_or(6433),
            db_pass: env::var("DB_PASS").unwrap_or_else(|_| "dev_pw".to_string()),
            output_dir: expand_tilde(
                &env::var("OUTPUT_DIR").unwrap_or_else(|_| "~/db_relations".to_string()),
            ),
            container_name: env::var("CONTAINER_NAME")
                .unwrap_or_else(|_| "ts_pg_av_dev".to_string()),
            pgadmin_port: env::var("PGADMIN_PORT")
                .unwrap_or_else(|_| "5050".to_string())
                .parse()
                .unwrap_or(5050),
            network_name: env::var("NETWORK_NAME").unwrap_or_else(|_| "av_network_dev".to_string()),
            compose_service: env::var("COMPOSE_SERVICE")
                .unwrap_or_else(|_| "av_timescaledb".to_string()),
            volume_prefix: env::var("VOLUME_PREFIX").unwrap_or_else(|_| "av_".to_string()),
        })
    }

    fn ensure_docker_files() -> anyhow::Result<()> {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        // Create docker-env.sh if it doesn't exist
        if !std::path::Path::new("docker-env.sh").exists() {
            let docker_env_content = r#"#!/bin/bash
# Docker environment setup script
# This loads environment variables for docker-compose

# Load the docker environment variables
source .env.docker

# Execute the passed command with the environment
exec "$@"
"#;
            fs::write("docker-env.sh", docker_env_content)?;

            // Make it executable
            let mut perms = fs::metadata("docker-env.sh")?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions("docker-env.sh", perms)?;
        }

        // Create .env.docker if it doesn't exist
        if !std::path::Path::new(".env.docker").exists() {
            let env_docker_content = r#"# Docker environment variables
# These are used by docker-compose

# Database configuration
export DB_NAME=sec_master
export DB_HOST=localhost
export DB_PORT=6433
export DB_USER=ts_user
export DB_PASS=dev_pw

# PgAdmin configuration
export PGADMIN_DEFAULT_EMAIL=admin@admin.com
export PGADMIN_DEFAULT_PASSWORD=admin
export PGADMIN_PORT=5050

# Container names and networks
export CONTAINER_NAME=ts_pg_av_dev
export NETWORK_NAME=av_network_dev

# Volume names
export PG_DATA_VOLUME=av_pg_data_dev
export PGADMIN_VOLUME=av_pgadmin_dev
export ARCHIVE_VOLUME=av_archive_dev
"#;
            fs::write(".env.docker", env_docker_content)?;
        }

        // Create docker-compose.yml if it doesn't exist
        if !std::path::Path::new("docker-compose.yml").exists() {
            let docker_compose_content = r#"version: '3.8'

services:
  av_timescaledb:
    image: timescale/timescaledb:latest-pg15
    container_name: ${CONTAINER_NAME:-ts_pg_av_dev}
    environment:
      POSTGRES_USER: ${DB_USER:-ts_user}
      POSTGRES_PASSWORD: ${DB_PASS:-dev_pw}
      POSTGRES_DB: ${DB_NAME:-sec_master}
    ports:
      - "${DB_PORT:-6433}:5432"
    volumes:
      - ${PG_DATA_VOLUME:-av_pg_data_dev}:/var/lib/postgresql/data
      - ${ARCHIVE_VOLUME:-av_archive_dev}:/archive
    networks:
      - ${NETWORK_NAME:-av_network_dev}
    restart: unless-stopped

  pgadmin:
    image: dpage/pgadmin4
    container_name: pgadmin_av_dev
    environment:
      PGADMIN_DEFAULT_EMAIL: ${PGADMIN_DEFAULT_EMAIL:-admin@admin.com}
      PGADMIN_DEFAULT_PASSWORD: ${PGADMIN_DEFAULT_PASSWORD:-admin}
    ports:
      - "${PGADMIN_PORT:-5050}:80"
    volumes:
      - ${PGADMIN_VOLUME:-av_pgadmin_dev}:/var/lib/pgadmin
    networks:
      - ${NETWORK_NAME:-av_network_dev}
    restart: unless-stopped

networks:
  av_network_dev:
    name: ${NETWORK_NAME:-av_network_dev}
    driver: bridge

volumes:
  av_pg_data_dev:
    name: ${PG_DATA_VOLUME:-av_pg_data_dev}
  av_pgadmin_dev:
    name: ${PGADMIN_VOLUME:-av_pgadmin_dev}
  av_archive_dev:
    name: ${ARCHIVE_VOLUME:-av_archive_dev}
"#;
            fs::write("docker-compose.yml", docker_compose_content)?;
        }

        Ok(())
    }
}
