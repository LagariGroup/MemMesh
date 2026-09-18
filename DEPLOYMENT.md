# MemMesh Deployment, Mesh Networking &amp; Operations Guide

This guide covers production self-hosting of the MemMesh daemon, secure cross-machine connectivity over private mesh networks, and backup/restore procedures.

\--------------------------------------------------------------------------------

## 1\. Network Topology &amp; Mesh Configuration

MemMesh is designed to avoid exposing open ports to the public internet. Instead, all traffic routes over an encrypted peer-to-peer mesh network (WireGuard-based) such as **NetBird** or **Tailscale**.

```text
  [Laptop: Dev Machine]             [Workstation: Secondary]
  NetBird IP: 100.64.0.10          NetBird IP: 100.64.0.12
           │                                 │
           └──────────────┬──────────────────┘
                          │ (Encrypted WireGuard Mesh)
                          ▼
            [Primary Host: Homelab Server / Pi]
                  NetBird IP: 100.64.0.5
              MemMesh Daemon: 100.64.0.5:8740

```

### Steps to Configure Mesh Connectivity

1. Install your mesh client (e.g., NetBird: `netbird up` or Tailscale: `tailscale up`) on the daemon host and all client machines.
2. Note the internal mesh IP of your daemon host (e.g., `100.64.0.5`).
3. In MemMesh's configuration, set: `MEMMESH_LISTEN_ADDR=100.64.0.5:8740` (or `0.0.0.0:8740`).
4. Ensure internal firewall rules (e.g., `ufw`) allow traffic on port 8740 exclusively across the mesh interface (`wt0` for NetBird or `tailscale0` for Tailscale):

```bash
sudo ufw allow in on wt0 to any port 8740 proto tcp

```

\--------------------------------------------------------------------------------

## 2\. Docker Compose Deployment (Recommended)

### `docker-compose.yml`

```yaml
version: '3.8'

services:
  memmesh:
    image: ghcr.io/memmesh/memmesh:latest
    container_name: memmesh-daemon
    restart: unless-stopped
    ports:
      # Expose only to local host and mesh interface
      - "8740:8740"
    environment:
      - MEMMESH_LISTEN_ADDR=0.0.0.0:8740
      - MEMMESH_DB_PATH=/data/memmesh.db
      - MEMMESH_VAULT_PATH=/data/vault
      - MEMMESH_AUTH_TOKEN=${MEMMESH_AUTH_TOKEN}
      - MEMMESH_LOG_LEVEL=info
      - MEMMESH_EMBEDDING_MODEL=bge-small-en-v1.5
      - MEMMESH_CONSOLIDATION_CRON=0 2 * * * # Nightly at 2:00 AM
    volumes:
      - ./data:/data
      - ./vault:/data/vault
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8740/health"]
      interval: 30s
      timeout: 5s
      retries: 3

```

### Environment Variables (`.env`)

```bash
MEMMESH_AUTH_TOKEN=generate-a-64-character-hex-string
MEMMESH_LISTEN_ADDR=0.0.0.0:8740
MEMMESH_LOG_LEVEL=info

```

\--------------------------------------------------------------------------------

## 3\. Native Linux Service Deployment (systemd)

For running natively on a Raspberry Pi 5, mini PC, or Linux server:

1. Download the binary:

```bash
sudo curl -Lo /usr/local/bin/memmesh https://github.com/memmesh/memmesh/releases/latest/download/memmesh-linux-arm64
sudo chmod +x /usr/local/bin/memmesh

```

1. Create dedicated system user and data directory:

```bash
sudo useradd -r -s /bin/false memmesh
sudo mkdir -p /var/lib/memmesh/vault
sudo chown -R memmesh:memmesh /var/lib/memmesh

```

1. Create systemd unit `/etc/systemd/system/memmesh.service`:

```ini
[Unit]
Description=MemMesh Agent Memory Daemon
After=network.target netbird.service tailscaled.service

[Service]
Type=simple
User=memmesh
Group=memmesh
WorkingDirectory=/var/lib/memmesh
Environment="MEMMESH_AUTH_TOKEN=your-token-here"
Environment="MEMMESH_DB_PATH=/var/lib/memmesh/memmesh.db"
Environment="MEMMESH_VAULT_PATH=/var/lib/memmesh/vault"
ExecStart=/usr/local/bin/memmesh daemon start --port 8740
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target

```

1. Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now memmesh

```

\--------------------------------------------------------------------------------

## 4\. Backups &amp; Disaster Recovery

### Option A: Continuous Streaming Backup with Litestream

SQLite's WAL mode allows real-time, transaction-level streaming backups with zero downtime.

Add a `litestream.yml` configuration:

```yaml
dbs:
  - path: /data/memmesh.db
    replicas:
      - type: s3
        bucket: my-homelab-backups
        path: memmesh/db
        endpoint: https://s3.amazonaws.com # Or MinIO / Cloudflare R2

```

### Option B: Obsidian / Git Vault Backup

The `.memmesh/vault/` directory is updated synchronously whenever semantic records are written. You can track this directory using Git:

```bash
cd /var/lib/memmesh/vault
git init
git remote add origin git@github.com:your-user/agent-memory-vault.git
# Use a systemd timer or cron job to commit and push daily

```

\--------------------------------------------------------------------------------

## 5\. Security &amp; Authentication Model

1. **Pre-Shared Bearer Tokens**:

   * Every request to `/v1/*` and `/sse` must include `Authorization: Bearer
