#!/bin/bash
# scripts/docker-env.sh

# Load environment variables
set -a
source .env.docker
set +a

# Execute the command
exec "$@"
