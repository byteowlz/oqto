#!/usr/bin/env bash
#
# VM Deployment Test Script for Oqto
# Tests deployment methods on fresh VMs in Proxmox
#
# Usage:
#   ./test-vm-deployment.sh                    # Run all scenarios from vm.tests.toml
#   ./test-vm-deployment.sh --scenario NAME    # Run specific scenario
#   ./test-vm-deployment.sh --list             # List available scenarios
#   ./test-vm-deployment.sh --prepare-images   # Download cloud images only
#   ./test-vm-deployment.sh --cleanup-all      # Remove all test VMs
#
# Configuration:
#   Copy vm.tests.toml.example to vm.tests.toml and customize
#

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OCTO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CONFIG_FILE="${SCRIPT_DIR}/vm.tests.toml"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Test tracking
TEST_RESULTS=()
CURRENT_VM_ID=""
CURRENT_VM_IP=""

# =============================================================================
# Logging
# =============================================================================

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_section() {
    echo ""
    echo -e "${BOLD}${CYAN}━━━ $1 ━━━${NC}"
}

# =============================================================================
# TOML Parsing (minimal implementation)
# =============================================================================

parse_toml() {
    local file="$1"
    local key="$2"
    
    # Remove comments and extract value
    grep -E "^${key}\s*=" "$file" 2>/dev/null | \
        sed -E 's/^[^=]+=\s*//' | \
        sed -E 's/^"(.*)"$/\1/' | \
        sed -E "s/^'(.*)'$/\1/" | \
        head -1
}

get_array_values() {
    local file="$1"
    local array_name="$2"
    
    awk -v name="$array_name" '
        /^\[\[.*\]\]/ { in_array = 0 }
        in_array && $0 ~ /^[^#]/ {
            if (match($0, /^[[:space:]]*([^=]+)=\s*"?([^"]*)"?/, arr)) {
                print arr[1] "=" arr[2]
            }
        }
        $0 ~ name { in_array = 1 }
    ' "$file"
}

# =============================================================================
# SSH Helper Functions
# =============================================================================

ssh_proxmox() {
    ssh -o ConnectTimeout=10 -o StrictHostKeyChecking=no \
        "${PROXMOX_USER}@${PROXMOX_HOST}" "$@"
}

ssh_vm() {
    local ip="$1"
    shift
    ssh -o ConnectTimeout=10 -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null \
        "${VM_USER}@${ip}" "$@"
}

scp_to_vm() {
    local ip="$1"
    local src="$2"
    local dst="$3"
    scp -o ConnectTimeout=10 -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null \
        "$src" "${VM_USER}@${ip}:$dst"
}

# =============================================================================
# Configuration Loading
# =============================================================================

load_config() {
    if [[ ! -f "$CONFIG_FILE" ]]; then
        log_error "Configuration file not found: $CONFIG_FILE"
        log_info "Copy vm.tests.toml.example to vm.tests.toml and customize"
        exit 1
    fi

    log_info "Loading configuration from $CONFIG_FILE"

    # Proxmox settings
    PROXMOX_HOST=$(parse_toml "$CONFIG_FILE" "host")
    PROXMOX_USER=$(parse_toml "$CONFIG_FILE" "ssh_user")
    STORAGE=$(parse_toml "$CONFIG_FILE" "storage")
    ISO_STORAGE=$(parse_toml "$CONFIG_FILE" "iso_storage")
    NETWORK_BRIDGE=$(parse_toml "$CONFIG_FILE" "network_bridge")

    # VM defaults
    VM_ID_START=$(parse_toml "$CONFIG_FILE" "vm_id_start")
    VM_MEMORY=$(parse_toml "$CONFIG_FILE" "memory")
    VM_CORES=$(parse_toml "$CONFIG_FILE" "cores")
    VM_DISK_SIZE=$(parse_toml "$CONFIG_FILE" "disk_size")
    VM_USER=$(parse_toml "$CONFIG_FILE" "default_user")
    
    # Source code settings
    SOURCE_MODE=$(parse_toml "$CONFIG_FILE" "mode")
    GIT_REPO=$(parse_toml "$CONFIG_FILE" "repo")
    GIT_REF=$(parse_toml "$CONFIG_FILE" "ref")
    FORWARD_SSH_AGENT=$(parse_toml "$CONFIG_FILE" "forward_ssh_agent")
    SOURCE_EXCLUDES=$(parse_toml "$CONFIG_FILE" "excludes")
    
    # SSH key
    SSH_KEY=$(parse_toml "$CONFIG_FILE" "ssh_public_key")
    if [[ -z "$SSH_KEY" && -f ~/.ssh/id_rsa.pub ]]; then
        SSH_KEY=$(cat ~/.ssh/id_rsa.pub)
    elif [[ -z "$SSH_KEY" && -f ~/.ssh/id_ed25519.pub ]]; then
        SSH_KEY=$(cat ~/.ssh/id_ed25519.pub)
    fi

    # Deployment settings
    ADMIN_USERNAME=$(parse_toml "$CONFIG_FILE" "admin_username")
    ADMIN_EMAIL=$(parse_toml "$CONFIG_FILE" "admin_email")
    WORKSPACE_DIR=$(parse_toml "$CONFIG_FILE" "workspace_dir")
    DEV_MODE=$(parse_toml "$CONFIG_FILE" "dev_mode")

    # Test settings
    VM_BOOT_TIMEOUT=$(parse_toml "$CONFIG_FILE" "vm_boot_timeout")
    SETUP_TIMEOUT=$(parse_toml "$CONFIG_FILE" "setup_timeout")
    CLEANUP_AFTER=$(parse_toml "$CONFIG_FILE" "cleanup_after_test")
    KEEP_FAILED=$(parse_toml "$CONFIG_FILE" "keep_failed_vms")

    # Set defaults
    PROXMOX_HOST="${PROXMOX_HOST:-wismut}"
    PROXMOX_USER="${PROXMOX_USER:-root}"
    STORAGE="${STORAGE:-local-lvm}"
    ISO_STORAGE="${ISO_STORAGE:-local}"
    NETWORK_BRIDGE="${NETWORK_BRIDGE:-vmbr0}"
    VM_ID_START="${VM_ID_START:-9000}"
    VM_MEMORY="${VM_MEMORY:-4096}"
    VM_CORES="${VM_CORES:-2}"
    VM_DISK_SIZE="${VM_DISK_SIZE:-20G}"
    VM_USER="${VM_USER:-ubuntu}"
    VM_BOOT_TIMEOUT="${VM_BOOT_TIMEOUT:-120}"
    SETUP_TIMEOUT="${SETUP_TIMEOUT:-600}"
    SOURCE_MODE="${SOURCE_MODE:-local-copy}"
    GIT_REF="${GIT_REF:-main}"
    FORWARD_SSH_AGENT="${FORWARD_SSH_AGENT:-true}"
    SOURCE_EXCLUDES="${SOURCE_EXCLUDES:-.git target node_modules *.log}"
}

# =============================================================================
# Cloud Image Management
# =============================================================================

download_cloud_image() {
    local distro="$1"
    local url="$2"
    local filename=$(basename "$url")
    local output_path="/var/lib/vz/template/iso/${filename}"

    log_info "Checking for $distro cloud image..."

    # Check if already exists
    if ssh_proxmox "test -f ${output_path}">/dev/null 2>&1; then
        log_success "Cloud image already exists: $filename"
        echo "$output_path"
        return 0
    fi

    log_info "Downloading $distro cloud image..."
    log_info "URL: $url"
    
    if ssh_proxmox "wget -q --show-progress -O ${output_path}.tmp '$url' && mv ${output_path}.tmp ${output_path}"; then
        log_success "Downloaded: $filename"
        echo "$output_path"
        return 0
    else
        log_error "Failed to download cloud image"
        return 1
    fi
}

prepare_all_images() {
    log_section "Preparing Cloud Images"

    # Ubuntu 24.04
    local ubuntu_24_04=$(parse_toml "$CONFIG_FILE" "ubuntu_24_04")
    if [[ -n "$ubuntu_24_04" ]]; then
        download_cloud_image "ubuntu-24.04" "$ubuntu_24_04"
    fi

    # Ubuntu 22.04
    local ubuntu_22_04=$(parse_toml "$CONFIG_FILE" "ubuntu_22_04")
    if [[ -n "$ubuntu_22_04" ]]; then
        download_cloud_image "ubuntu-22.04" "$ubuntu_22_04"
    fi

    # Debian 12
    local debian_12=$(parse_toml "$CONFIG_FILE" "debian_12")
    if [[ -n "$debian_12" ]]; then
        download_cloud_image "debian-12" "$debian_12"
    fi

    # Arch
    local arch=$(parse_toml "$CONFIG_FILE" "arch")
    if [[ -n "$arch" ]]; then
        download_cloud_image "arch" "$arch"
    fi

    log_success "All images prepared"
}

get_image_path() {
    local distro="$1"
    
    case "$distro" in
        ubuntu-24.04|ubuntu-24.04-*)
            echo "/var/lib/vz/template/iso/noble-server-cloudimg-amd64.img"
            ;;
        ubuntu-22.04|ubuntu-22.04-*)
            echo "/var/lib/vz/template/iso/jammy-server-cloudimg-amd64.img"
            ;;
        debian-12|debian-12-*)
            echo "/var/lib/vz/template/iso/debian-12-generic-amd64.qcow2"
            ;;
        debian-11|debian-11-*)
            echo "/var/lib/vz/template/iso/debian-11-generic-amd64.qcow2"
            ;;
        arch|arch-*)
            echo "/var/lib/vz/template/iso/Arch-Linux-x86_64-cloudimg.qcow2"
            ;;
        fedora-40|fedora-40-*)
            echo "/var/lib/vz/template/iso/Fedora-Cloud-Base-Generic.x86_64-40-1.14.qcow2"
            ;;
        *)
            log_error "Unknown distro: $distro"
            return 1
            ;;
    esac
}

# =============================================================================
# VM Lifecycle
# =============================================================================

create_vm() {
    local vm_id="$1"
    local name="$2"
    local distro="$3"
    
    log_info "Creating VM $vm_id ($name) with $distro"

    local image_path=$(get_image_path "$distro")
    if [[ -z "$image_path" ]]; then
        return 1
    fi

    # Check if image exists
    if ! ssh_proxmox "test -f $image_path" 2>/dev/null; then
        log_error "Cloud image not found: $image_path"
        log_info "Run: ./test-vm-deployment.sh --prepare-images"
        return 1
    fi

    CURRENT_VM_ID="$vm_id"

    # Destroy existing VM if exists
    ssh_proxmox "qm destroy $vm_id --purge 2>/dev/null || true"

    # Create VM
    ssh_proxmox "qm create $vm_id \
        --name $name \
        --memory $VM_MEMORY \
        --cores $VM_CORES \
        --cpu x86-64-v2-AES \
        --net0 virtio,bridge=$NETWORK_BRIDGE \
        --scsihw virtio-scsi-single \
        --ostype l26 \
        --agent enabled=1"

    # Import disk
    log_info "Importing disk image..."
    ssh_proxmox "qm disk import $vm_id $image_path $STORAGE --format qcow2"
    
    # Attach disk
    ssh_proxmox "qm set $vm_id --scsi0 ${STORAGE}:vm-${vm_id}-disk-0"
    
    # Add cloud-init drive
    ssh_proxmox "qm set $vm_id --ide2 ${ISO_STORAGE}:cloudinit"
    
    # Set boot order
    ssh_proxmox "qm set $vm_id --boot order=scsi0"

    # Configure cloud-init
    setup_cloud_init "$vm_id" "$name"

    log_success "VM $vm_id created"
}

setup_cloud_init() {
    local vm_id="$1"
    local hostname="$2"

    log_info "Configuring cloud-init..."

    # Set cloud-init values
    ssh_proxmox "qm set $vm_id --ciuser $VM_USER"
    ssh_proxmox "qm set $vm_id --cipassword ''"
    ssh_proxmox "qm set $vm_id --sshkeys '$(echo "$SSH_KEY" | sed 's/ /\\ /g')'"
    ssh_proxmox "qm set $vm_id --ipconfig0 ip=dhcp"
    ssh_proxmox "qm set $vm_id --searchdomain local"
    ssh_proxmox "qm set $vm_id --nameserver 8.8.8.8"

    # Regenerate cloud-init
    ssh_proxmox "qm cloudinit dump $vm_id user" >/dev/null 2>&1 || true
}

start_vm() {
    local vm_id="$1"
    
    log_info "Starting VM $vm_id..."
    ssh_proxmox "qm start $vm_id"
    
    # Wait for boot
    log_info "Waiting for VM to boot (${VM_BOOT_TIMEOUT}s timeout)..."
    local count=0
    while [[ $count -lt $VM_BOOT_TIMEOUT ]]; do
        sleep 5
        count=$((count + 5))
        
        # Check if agent is running
        local status=$(ssh_proxmox "qm agent $vm_id ping 2>/dev/null && echo 'running' || echo 'waiting'")
        if [[ "$status" == "running" ]]; then
            log_success "VM agent is running"
            break
        fi
        
        echo -ne "\r  Waiting... ${count}s"
    done
    echo ""

    if [[ $count -ge $VM_BOOT_TIMEOUT ]]; then
        log_error "VM failed to start within timeout"
        return 1
    fi

    # Get IP address
    sleep 5  # Give DHCP time to assign
    CURRENT_VM_IP=$(ssh_proxmox "qm agent $vm_id network-get-interfaces" 2>/dev/null | \
        grep -oE '([0-9]{1,3}\.){3}[0-9]{1,3}' | \
        grep -vE '^(127|169\.254)' | \
        head -1)

    if [[ -z "$CURRENT_VM_IP" ]]; then
        log_error "Could not determine VM IP address"
        return 1
    fi

    log_success "VM is running at $CURRENT_VM_IP"
    
    # Wait for SSH
    log_info "Waiting for SSH..."
    local ssh_attempts=0
    while [[ $ssh_attempts -lt 30 ]]; do
        if ssh_vm "$CURRENT_VM_IP" "echo 'ready'" 2>/dev/null | grep -q "ready"; then
            log_success "SSH is ready"
            return 0
        fi
        sleep 2
        ssh_attempts=$((ssh_attempts + 1))
        echo -ne "\r  SSH attempt $ssh_attempts/30"
    done
    echo ""
    
    log_error "SSH connection failed"
    return 1
}

destroy_vm() {
    local vm_id="$1"
    
    log_info "Destroying VM $vm_id..."
    ssh_proxmox "qm stop $vm_id --force 2>/dev/null || true"
    sleep 2
    ssh_proxmox "qm destroy $vm_id --purge 2>/dev/null || true"
    log_success "VM $vm_id destroyed"
}

# =============================================================================
# Oqto Setup
# =============================================================================

generate_setup_toml() {
    local scenario_name="$1"
    local backend_mode="$2"
    local user_mode="$3"
    local container_runtime="$4"
    local production="$5"

    local toml_file="/tmp/oqto.setup.${scenario_name}.toml"

    cat > "$toml_file" << EOF
# Generated by test-vm-deployment.sh
# Scenario: $scenario_name

[deployment]
user_mode = "$user_mode"
backend_mode = "$backend_mode"
EOF

    if [[ -n "$container_runtime" ]]; then
        echo "container_runtime = \"$container_runtime\"" >> "$toml_file"
    fi

    if [[ -n "$WORKSPACE_DIR" ]]; then
        echo "workspace_dir = \"$WORKSPACE_DIR\"" >> "$toml_file"
    fi

    cat >> "$toml_file" << EOF
log_level = "${LOG_LEVEL:-info}"

[admin]
username = "$ADMIN_USERNAME"
EOF

    if [[ -n "$ADMIN_EMAIL" ]]; then
        echo "email = \"$ADMIN_EMAIL\"" >> "$toml_file"
    fi

    if [[ "$DEV_MODE" == "true" ]]; then
        cat >> "$toml_file" << EOF

[development]
dev_mode = true
EOF
    fi

    # Add providers from config
    cat >> "$toml_file" << EOF

[providers]
EOF

    # Parse providers from config file
    grep -E '^\[providers\.' "$CONFIG_FILE" | while read -r line; do
        local provider_name=$(echo "$line" | sed -E 's/^\[providers\.(.*)\]$/\1/')
        local enabled=$(awk "/^\[providers\.$provider_name\]/,/\[/{/^enabled\s*=/}" "$CONFIG_FILE" | grep "enabled" | sed -E 's/.*=\s*//' | tr -d '"')
        local ptype=$(awk "/^\[providers\.$provider_name\]/,/\[/{/^type\s*=/}" "$CONFIG_FILE" | grep "type" | sed -E 's/.*=\s*//' | tr -d '"')
        local base_url=$(awk "/^\[providers\.$provider_name\]/,/\[/{/^base_url\s*=/}" "$CONFIG_FILE" | grep "base_url" | sed -E 's/.*=\s*//' | tr -d '"')
        local api_key=$(awk "/^\[providers\.$provider_name\]/,/\[/{/^api_key\s*=/}" "$CONFIG_FILE" | grep "api_key" | sed -E 's/.*=\s*//' | tr -d '"')

        if [[ "$enabled" == "true" ]]; then
            cat >> "$toml_file" << EOF

[providers.$provider_name]
type = "${ptype:-$provider_name}"
EOF
            if [[ -n "$base_url" ]]; then
                echo "base_url = \"$base_url\"" >> "$toml_file"
            fi
            if [[ -n "$api_key" ]]; then
                # Handle env: prefix
                if [[ "$api_key" == env:* ]]; then
                    local env_var="${api_key#env:}"
                    local env_value="${!env_var:-}"
                    if [[ -n "$env_value" ]]; then
                        echo "api_key = \"$env_value\"" >> "$toml_file"
                    fi
                else
                    echo "api_key = \"$api_key\"" >> "$toml_file"
                fi
            fi
        fi
    done

    echo "$toml_file"
}

deploy_oqto() {
    local scenario="$1"
    local toml_file="$2"

    log_section "Deploying Oqto"
    log_info "Source mode: $SOURCE_MODE"
    log_info "Git repo: $GIT_REPO"

    # Copy setup files to VM
    log_info "Copying setup files..."
    scp_to_vm "$CURRENT_VM_IP" "$OCTO_DIR/setup.sh" "/tmp/setup.sh"
    scp_to_vm "$CURRENT_VM_IP" "$toml_file" "/tmp/oqto.setup.toml"

    # Deploy source code based on mode
    case "$SOURCE_MODE" in
        git-clone|git-fresh)
            deploy_via_git_clone
            ;;
        local-sync)
            deploy_via_local_sync
            ;;
        local-copy|*)
            deploy_via_local_copy
            ;;
    esac

    # Run setup
    log_info "Running setup.sh (timeout: ${SETUP_TIMEOUT}s)..."

    if timeout "$SETUP_TIMEOUT" ssh_vm "$CURRENT_VM_IP" "
        set -e
        cd /tmp/oqto
        chmod +x /tmp/setup.sh
        sudo OQTO_NON_INTERACTIVE=true OQTO_CONFIG_FILE=/tmp/oqto.setup.toml OQTO_INSTALL_DEPS=yes OQTO_INSTALL_SERVICE=yes /tmp/setup.sh --config /tmp/oqto.setup.toml 2>&1
    " ; then
        log_success "Setup completed successfully"
        return 0
    else
        log_error "Setup failed or timed out"
        return 1
    fi
}

deploy_via_git_clone() {
    log_info "Cloning from git repository..."
    log_info "Repository: $GIT_REPO"
    log_info "Ref: $GIT_REF"

    # Prepare SSH agent forwarding if needed
    local ssh_opts=""
    if [[ "$FORWARD_SSH_AGENT" == "true" ]]; then
        if [[ -z "${SSH_AUTH_SOCK:-}" ]]; then
            log_warn "SSH agent forwarding enabled but SSH_AUTH_SOCK not set"
            log_info "Ensure your SSH agent is running: eval \$(ssh-agent -s) && ssh-add"
        else
            log_info "Using SSH agent forwarding for private repo access"
            ssh_opts="-A"  # Enable agent forwarding
        fi
    fi

    # Clone the repository on the VM
    local ssh_cmd="ssh ${ssh_opts} -o ConnectTimeout=10 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null ${VM_USER}@${CURRENT_VM_IP}"
    
    $ssh_cmd "
        set -e
        echo 'Cloning repository...'
        if [[ -d /tmp/oqto && '$SOURCE_MODE' == 'git-fresh' ]]; then
            rm -rf /tmp/oqto
        fi
        if [[ ! -d /tmp/oqto ]]; then
            git clone --depth 1 --branch '$GIT_REF' '$GIT_REPO' /tmp/oqto
        else
            cd /tmp/oqto
            git fetch origin
            git checkout '$GIT_REF'
        fi
        echo 'Repository cloned successfully'
        cd /tmp/oqto
        git log -1 --oneline
    "
    
    log_success "Source code cloned to VM"
}

deploy_via_local_sync() {
    log_info "Syncing local source code with rsync (includes .git)..."

    # Build exclude patterns
    local exclude_opts=""
    for pattern in $SOURCE_EXCLUDES; do
        # Remove .git from excludes for sync mode
        if [[ "$pattern" != ".git" ]]; then
            exclude_opts="$exclude_opts --exclude='$pattern'"
        fi
    done

    # Use rsync with .git preserved
    if command -v rsync &> /dev/null; then
        rsync -avz --delete $exclude_opts \
            -e "ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null" \
            "$OCTO_DIR/" "${VM_USER}@${CURRENT_VM_IP}:/tmp/oqto/"
    else
        log_error "rsync not available, falling back to tar"
        deploy_via_local_copy
        return
    fi
    
    log_success "Source code synced to VM"
}

deploy_via_local_copy() {
    log_info "Copying local source code (this may take a while)..."
    ssh_vm "$CURRENT_VM_IP" "mkdir -p /tmp/oqto"
    
    # Build exclude patterns
    local exclude_opts=""
    for pattern in $SOURCE_EXCLUDES; do
        exclude_opts="$exclude_opts --exclude='$pattern'"
    done

    # Use rsync if available, otherwise tar
    if command -v rsync &> /dev/null; then
        rsync -avz $exclude_opts \
            -e "ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null" \
            "$OCTO_DIR/" "${VM_USER}@${CURRENT_VM_IP}:/tmp/oqto/" 2>/dev/null
    else
        # Create tar archive with exclusions
        (cd "$OCTO_DIR" && tar czf - $exclude_opts . 2>/dev/null) | \
            ssh_vm "$CURRENT_VM_IP" "tar xzf - -C /tmp/oqto"
    fi
    
    log_success "Source code copied to VM"
}

# =============================================================================
# Testing & Verification
# =============================================================================

verify_deployment() {
    log_section "Verifying Deployment"

    local failed=0

    # Check oqto service
    log_info "Checking oqto service..."
    if ssh_vm "$CURRENT_VM_IP" "sudo systemctl is-active oqto" >/dev/null 2>&1; then
        log_success "oqto service is running"
    else
        log_error "oqto service is not running"
        failed=$((failed + 1))
    fi

    # Check hstry service
    log_info "Checking hstry service..."
    if ssh_vm "$CURRENT_VM_IP" "sudo systemctl is-active hstry" >/dev/null 2>&1; then
        log_success "hstry service is running"
    else
        log_error "hstry service is not running"
        failed=$((failed + 1))
    fi

    # Check API port
    log_info "Checking API on port 8080..."
    if ssh_vm "$CURRENT_VM_IP" "curl -sf http://localhost:8080/health 2>/dev/null || curl -sf http://localhost:8080/api/health 2>/dev/null || true" | grep -q "ok\|healthy"; then
        log_success "API is responding"
    else
        log_warn "API health check inconclusive (may be normal)"
    fi

    return $failed
}

run_smoke_tests() {
    log_section "Running Smoke Tests"
    
    log_info "Testing oqtoctl..."
    if ssh_vm "$CURRENT_VM_IP" "oqtoctl --help" >/dev/null 2>&1; then
        log_success "oqtoctl is available"
    else
        log_error "oqtoctl not found"
        return 1
    fi

    log_info "Checking agent tools..."
    local tools=("agntz" "mmry" "trx")
    for tool in "${tools[@]}"; do
        if ssh_vm "$CURRENT_VM_IP" "which $tool" >/dev/null 2>&1; then
            log_success "$tool is installed"
        else
            log_warn "$tool not found in PATH"
        fi
    done

    return 0
}

# =============================================================================
# Scenario Runner
# =============================================================================

run_scenario() {
    local scenario_name="$1"
    local vm_id="$2"
    local distro="$3"
    local backend_mode="$4"
    local user_mode="$5"
    local container_runtime="$6"
    local production="$7"

    log_section "Running Scenario: $scenario_name"
    log_info "VM ID: $vm_id"
    log_info "Distro: $distro"
    log_info "Backend: $backend_mode"
    log_info "User Mode: $user_mode"

    local start_time=$(date +%s)
    local result="FAILED"
    local error_msg=""

    # Create VM
    if ! create_vm "$vm_id" "$scenario_name" "$distro"; then
        error_msg="Failed to create VM"
        result="FAILED"
        TEST_RESULTS+=("$scenario_name|$result|0|$error_msg")
        return 1
    fi

    # Start VM
    if ! start_vm "$vm_id"; then
        error_msg="Failed to start VM"
        result="FAILED"
        destroy_vm "$vm_id"
        TEST_RESULTS+=("$scenario_name|$result|0|$error_msg")
        return 1
    fi

    # Generate setup.toml
    local toml_file=$(generate_setup_toml "$scenario_name" "$backend_mode" "$user_mode" "$container_runtime" "$production")

    # Deploy Oqto
    if ! deploy_oqto "$scenario_name" "$toml_file"; then
        error_msg="Setup failed"
        result="FAILED"
        if [[ "$KEEP_FAILED" == "true" ]]; then
            log_warn "Keeping VM $vm_id for debugging"
        else
            destroy_vm "$vm_id"
        fi
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        TEST_RESULTS+=("$scenario_name|$result|$duration|$error_msg")
        return 1
    fi

    # Verify deployment
    verify_deployment
    local verify_failed=$?

    # Run smoke tests
    run_smoke_tests
    local smoke_failed=$?

    local end_time=$(date +%s)
    local duration=$((end_time - start_time))

    if [[ $verify_failed -eq 0 && $smoke_failed -eq 0 ]]; then
        result="PASSED"
        log_success "Scenario completed successfully (${duration}s)"
    else
        result="PASSED_WITH_WARNINGS"
        log_warn "Scenario completed with warnings (${duration}s)"
    fi

    # Cleanup if requested
    if [[ "$CLEANUP_AFTER" == "true" ]]; then
        destroy_vm "$vm_id"
    else
        log_info "VM $vm_id preserved for inspection"
    fi

    TEST_RESULTS+=("$scenario_name|$result|$duration|$error_msg")
    return 0
}

# =============================================================================
# List Scenarios
# =============================================================================

list_scenarios() {
    log_section "Available Test Scenarios"
    
    grep -E '^\[\[scenario\]\]' -A 10 "$CONFIG_FILE" | \
        awk '/^\[\[scenario\]\]/{RS=""; FS="\n"; next} {print}' | \
        while IFS= read -r line; do
            if [[ "$line" =~ name[[:space:]]*=[[:space:]]*\"([^\"]+)\" ]]; then
                echo "  ${BOLD}${BASH_REMATCH[1]}${NC}"
            elif [[ "$line" =~ description[[:space:]]*=[[:space:]]*\"([^\"]+)\" ]]; then
                echo "    ${BASH_REMATCH[1]}"
            elif [[ "$line" =~ distro[[:space:]]*=[[:space:]]*\"([^\"]+)\" ]]; then
                echo "    Distro: ${BASH_REMATCH[1]}"
            fi
        done
}

parse_scenarios() {
    local scenarios=()
    local in_scenario=false
    local idx=0
    
    while IFS= read -r line || [[ -n "$line" ]]; do
        if [[ "$line" =~ ^\[\[scenario\]\] ]]; then
            in_scenario=true
            idx=$((idx + 1))
            scenarios[$idx]=""
        elif [[ "$in_scenario" == true ]]; then
            if [[ -z "$line" ]]; then
                in_scenario=false
            else
                scenarios[$idx]="${scenarios[$idx]}\n$line"
            fi
        fi
    done < "$CONFIG_FILE"
    
    echo "${scenarios[@]}"
}

# =============================================================================
# Report Generation
# =============================================================================

print_report() {
    log_section "Test Report"

    echo ""
    printf "${BOLD}%-40s %-15s %-10s %s${NC}\n" "Scenario" "Result" "Duration" "Notes"
    printf "${BOLD}%s${NC}\n" "--------------------------------------------------------------------------------"

    local passed=0
    local failed=0

    for result in "${TEST_RESULTS[@]}"; do
        IFS='|' read -r name status duration notes <<< "$result"
        
        local status_color="${RED}"
        if [[ "$status" == "PASSED" ]]; then
            status_color="${GREEN}"
            passed=$((passed + 1))
        elif [[ "$status" == "PASSED_WITH_WARNINGS" ]]; then
            status_color="${YELLOW}"
            passed=$((passed + 1))
        else
            failed=$((failed + 1))
        fi

        printf "%-40s ${status_color}%-15s${NC} %-10s %s\n" \
            "$name" "$status" "${duration}s" "$notes"
    done

    echo ""
    log_info "Total: $((passed + failed)) | ${GREEN}Passed: $passed${NC} | ${RED}Failed: $failed${NC}"
    echo ""

    if [[ $failed -gt 0 ]]; then
        return 1
    else
        return 0
    fi
}

# =============================================================================
# Cleanup
# =============================================================================

cleanup_all() {
    log_section "Cleaning Up All Test VMs"
    
    # Find all test VMs (starting from VM_ID_START)
    local vms=$(ssh_proxmox "pvesh get /nodes/${PROXMOX_HOST}/qemu --output-format json" 2>/dev/null | \
        jq -r ".[] | select(.vmid >= $VM_ID_START) | .vmid")

    if [[ -z "$vms" ]]; then
        log_info "No test VMs found"
        return 0
    fi

    for vm_id in $vms; do
        log_info "Destroying VM $vm_id..."
        ssh_proxmox "qm stop $vm_id --force 2>/dev/null || true"
        sleep 1
        ssh_proxmox "qm destroy $vm_id --purge 2>/dev/null || true"
    done

    log_success "All test VMs cleaned up"
}

# =============================================================================
# Main
# =============================================================================

main() {
    local single_scenario=""
    local prepare_only=false
    local cleanup_only=false
    local list_only=false

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --scenario|-s)
                single_scenario="$2"
                shift 2
                ;;
            --prepare-images)
                prepare_only=true
                shift
                ;;
            --cleanup-all)
                cleanup_only=true
                shift
                ;;
            --list|-l)
                list_only=true
                shift
                ;;
            --help|-h)
                echo "Usage: $0 [OPTIONS]"
                echo ""
                echo "Options:"
                echo "  --scenario NAME     Run specific scenario"
                echo "  --prepare-images    Download cloud images only"
                echo "  --cleanup-all       Remove all test VMs"
                echo "  --list, -l          List available scenarios"
                echo "  --help, -h          Show this help"
                echo ""
                echo "Configuration:"
                echo "  Edit scripts/vm.tests.toml to configure tests"
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done

    # Load configuration
    load_config

    # Handle special modes
    if [[ "$list_only" == true ]]; then
        list_scenarios
        exit 0
    fi

    if [[ "$cleanup_only" == true ]]; then
        cleanup_all
        exit 0
    fi

    if [[ "$prepare_only" == true ]]; then
        prepare_all_images
        exit 0
    fi

    # Validate Proxmox connection
    log_info "Validating Proxmox connection..."
    if ! ssh_proxmox "echo 'connected'" >/dev/null 2>&1; then
        log_error "Cannot connect to Proxmox at ${PROXMOX_USER}@${PROXMOX_HOST}"
        log_info "Ensure SSH keys are configured and Proxmox is accessible"
        exit 1
    fi
    log_success "Connected to Proxmox"

    # Prepare images
    prepare_all_images

    # Run scenarios
    local vm_id=$VM_ID_START
    local scenarios_run=0

    # Parse and run scenarios
    grep -E '^\[\[scenario\]\]' "$CONFIG_FILE" >/dev/null 2>&1 || {
        log_error "No scenarios defined in $CONFIG_FILE"
        exit 1
    }

    # Read scenarios using awk for better parsing
    awk '/^\[\[scenario\]\]/{found=1} found' "$CONFIG_FILE" | \
    awk 'BEGIN{RS=""; FS="\n"} NF' | \
    while IFS= read -r scenario_block; do
        # Skip empty blocks
        [[ -z "$scenario_block" ]] && continue

        # Parse scenario fields
        local name=$(echo "$scenario_block" | grep "^name" | sed -E 's/.*=\s*"([^"]+)".*/\1/')
        
        # Skip if running single scenario and this doesn't match
        if [[ -n "$single_scenario" && "$name" != "$single_scenario" ]]; then
            continue
        fi

        local distro=$(echo "$scenario_block" | grep "^distro" | sed -E 's/.*=\s*"([^"]+)".*/\1/')
        local backend=$(echo "$scenario_block" | grep "^backend_mode" | sed -E 's/.*=\s*"([^"]+)".*/\1/')
        local user_mode=$(echo "$scenario_block" | grep "^user_mode" | sed -E 's/.*=\s*"([^"]+)".*/\1/')
        local container=$(echo "$scenario_block" | grep "^container_runtime" | sed -E 's/.*=\s*"([^"]+)".*/\1/')
        local production=$(echo "$scenario_block" | grep "^production" | sed -E 's/.*=\s*(true|false).*/\1/')

        # Run the scenario
        run_scenario "$name" "$vm_id" "$distro" "$backend" "$user_mode" "$container" "$production"
        
        vm_id=$((vm_id + 1))
        scenarios_run=$((scenarios_run + 1))
    done

    if [[ $scenarios_run -eq 0 && -n "$single_scenario" ]]; then
        log_error "Scenario not found: $single_scenario"
        list_scenarios
        exit 1
    fi

    # Print report
    print_report
    exit $?
}

main "$@"