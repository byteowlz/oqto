#!/usr/bin/env bash
# Helper script for managing Caddy routes via Admin API
# This demonstrates the API calls the Rust backend will make

CADDY_ADMIN="${CADDY_ADMIN:-http://localhost:41802}"

# Add routes for a container
# Usage: add_container_routes <container_id> <container_hostname>
add_container_routes() {
    local id="$1"
    local hostname="$2"
    
    if [[ -z "$id" || -z "$hostname" ]]; then
        echo "Usage: $0 add <container_id> <container_hostname>"
        exit 1
    fi

    echo "Adding routes for container: $id -> $hostname"
    
    # Create route configuration for this container
    # Use /config/apps/http/servers/srv0/routes/0 to insert at beginning (before fallback)
    cat <<EOF | curl -s -X POST "${CADDY_ADMIN}/config/apps/http/servers/srv0/routes/0" \
        -H "Content-Type: application/json" \
        -d @-
{
    "@id": "container-${id}",
    "match": [
        {
            "path": ["/c/${id}/*"]
        }
    ],
    "handle": [
        {
            "handler": "subroute",
            "routes": [
                {
                    "match": [{"path": ["/c/${id}/api/*"]}],
                    "handle": [
                        {
                            "handler": "rewrite",
                            "strip_path_prefix": "/c/${id}/api"
                        },
                        {
                            "handler": "reverse_proxy",
                            "upstreams": [{"dial": "${hostname}:41820"}]
                        }
                    ]
                },
                {
                    "match": [{"path": ["/c/${id}/files/*"]}],
                    "handle": [
                        {
                            "handler": "rewrite",
                            "strip_path_prefix": "/c/${id}/files"
                        },
                        {
                            "handler": "reverse_proxy",
                            "upstreams": [{"dial": "${hostname}:41821"}]
                        }
                    ]
                },
                {
                    "match": [{"path": ["/c/${id}/term/*"]}],
                    "handle": [
                        {
                            "handler": "rewrite",
                            "strip_path_prefix": "/c/${id}/term"
                        },
                        {
                            "handler": "reverse_proxy",
                            "upstreams": [{"dial": "${hostname}:41822"}],
                            "transport": {
                                "protocol": "http",
                                "versions": ["1.1", "2"]
                            }
                        }
                    ]
                }
            ]
        }
    ]
}
EOF
    echo ""
}

# Remove routes for a container
# Usage: remove_container_routes <container_id>
remove_container_routes() {
    local id="$1"
    
    if [[ -z "$id" ]]; then
        echo "Usage: $0 remove <container_id>"
        exit 1
    fi

    echo "Removing routes for container: $id"
    curl -s -X DELETE "${CADDY_ADMIN}/id/container-${id}"
    echo ""
}

# List all routes
list_routes() {
    echo "Current Caddy configuration:"
    curl -s "${CADDY_ADMIN}/config/" | jq .
}

# Show usage
usage() {
    cat <<EOF
Caddy Route Manager for Octo Containers

Usage:
    $0 add <container_id> <container_hostname>    Add routes for a container
    $0 remove <container_id>                      Remove routes for a container
    $0 list                                       List current configuration

Examples:
    $0 add user123-session1 octo-user123-session1
    $0 remove user123-session1
    $0 list

Environment:
    CADDY_ADMIN    Caddy admin API URL (default: http://localhost:41802)
EOF
}

# Main
case "$1" in
    add)
        add_container_routes "$2" "$3"
        ;;
    remove)
        remove_container_routes "$2"
        ;;
    list)
        list_routes
        ;;
    *)
        usage
        exit 1
        ;;
esac
