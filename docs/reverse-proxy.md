# Reverse Proxy Setup

Mailquill serves everything — the SPA frontend, the `/api` routes, and the
Server-Sent Events stream — from a single HTTP port. A reverse proxy in front
terminates TLS and forwards plain HTTP to that one upstream. No WebSockets are
used; live updates arrive via SSE on `GET /api/events`.

All examples below assume:

- Public hostname: `mail.example.com`
- Upstream: `127.0.0.1:8080` (the Docker Compose default; native installs
  default to `8765`, configurable via `SERVER_HOST` / `SERVER_PORT`)

## Requirements for Any Proxy

Whatever proxy you pick, the same five rules apply:

1. **Set `APP_BASE_URL` to the public URL** (e.g. `https://mail.example.com`).
   Mailquill builds OAuth redirect URIs from this value — it does not derive
   the external URL from `X-Forwarded-*` headers.
2. **Do not buffer `/api/events`.** It is a long-lived SSE stream
   (`text/event-stream`). Response buffering or compression on this path
   delays or breaks live updates.
3. **Use generous read/idle timeouts** on the SSE path. The stream sends
   keep-alive events, but short proxy timeouts will still cut it off and force
   reconnects.
4. **Raise the request body limit** so large attachment uploads pass through
   (`client_max_body_size` and friends). Mailquill validates sizes itself; the
   proxy limit just needs to be at least as large.
5. **Bind Mailquill to loopback** (`SERVER_HOST=127.0.0.1`) or an internal
   Docker network so it is only reachable through the proxy.

Health check endpoint for probes: `GET /api/health` → `{"status":"ok"}`.

Note: Mailquill sets its own `Content-Security-Policy` header. Do not override
or append CSP at the proxy.

## nginx

```nginx
upstream mailquill {
    server 127.0.0.1:8080;
    keepalive 8;
}

server {
    listen 443 ssl;
    http2 on;
    server_name mail.example.com;

    ssl_certificate     /etc/letsencrypt/live/mail.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/mail.example.com/privkey.pem;

    client_max_body_size 50m;

    location / {
        proxy_pass http://mailquill;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # SSE stream: no buffering, long read timeout
    location /api/events {
        proxy_pass http://mailquill;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_buffering off;
        proxy_cache off;
        gzip off;
        proxy_read_timeout 1h;
        proxy_send_timeout 1h;
    }
}

server {
    listen 80;
    server_name mail.example.com;
    return 301 https://$host$request_uri;
}
```

## Caddy

Caddy handles TLS issuance automatically and streams SSE responses without
extra configuration (flush intervals are automatic for `text/event-stream`).

```caddyfile
mail.example.com {
    request_body {
        max_size 50MB
    }

    reverse_proxy 127.0.0.1:8080 {
        # Disable response buffering so the SSE stream flushes immediately
        flush_interval -1
    }
}
```

## Traefik

Traefik v3 does not buffer responses by default, so SSE works out of the box.
Timeouts only need attention if you set `respondingTimeouts` globally — in
that case exempt or raise `readTimeout` for the SSE stream.

### Docker labels

Attach these labels to the Mailquill service in `docker-compose.yml` and put
the container on the same network as Traefik (drop the `ports:` mapping so the
app is only reachable through the proxy):

```yaml
services:
  app:
    image: ghcr.io/mailquill/mailquill:latest
    networks:
      - traefik
    labels:
      - traefik.enable=true
      - traefik.http.routers.mailquill.rule=Host(`mail.example.com`)
      - traefik.http.routers.mailquill.entrypoints=websecure
      - traefik.http.routers.mailquill.tls=true
      - traefik.http.routers.mailquill.tls.certresolver=letsencrypt
      - traefik.http.services.mailquill.loadbalancer.server.port=8080

networks:
  traefik:
    external: true
```

### File provider

For a Mailquill instance running outside Docker (e.g. on the host):

```yaml
# dynamic/mailquill.yml
http:
  routers:
    mailquill:
      rule: Host(`mail.example.com`)
      entryPoints:
        - websecure
      tls: {}
      service: mailquill
  services:
    mailquill:
      loadBalancer:
        servers:
          - url: http://host.docker.internal:8080
```

## HAProxy

HAProxy needs `timeout tunnel` for the SSE stream — without it,
`timeout server` closes the connection.

```haproxy
defaults
    mode http
    timeout connect 5s
    timeout client  30s
    timeout server  30s
    # Applies once a connection is upgraded/streaming (covers SSE)
    timeout tunnel  1h

frontend https-in
    bind :443 ssl crt /etc/haproxy/certs/mail.example.com.pem
    bind :80
    http-request redirect scheme https unless { ssl_fc }
    http-request set-header X-Forwarded-Proto https
    http-request set-header X-Forwarded-For %[src]
    default_backend mailquill

backend mailquill
    option httpchk GET /api/health
    server app1 127.0.0.1:8080 check
```

HAProxy does not buffer full responses, so SSE events pass through as they are
written. Request body size is unlimited by default; if you use
`http-request deny if { req.body_size gt ... }` rules elsewhere, exempt the
attachment upload routes.

## Envoy

Envoy streams responses by default. The two settings that matter are the
route-level idle timeout for SSE and the request body limit (Envoy's buffer
filter, if enabled, must allow attachment-sized bodies).

```yaml
static_resources:
  listeners:
    - name: https
      address:
        socket_address: { address: 0.0.0.0, port_value: 443 }
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /etc/envoy/certs/fullchain.pem }
                    private_key: { filename: /etc/envoy/certs/privkey.pem }
          filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: mailquill
                route_config:
                  virtual_hosts:
                    - name: mailquill
                      domains: ["mail.example.com"]
                      routes:
                        # SSE stream: disable idle timeout for this route
                        - match: { path: "/api/events" }
                          route:
                            cluster: mailquill
                            idle_timeout: 0s
                            timeout: 0s
                        - match: { prefix: "/" }
                          route:
                            cluster: mailquill
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router

  clusters:
    - name: mailquill
      type: STRICT_DNS
      load_assignment:
        cluster_name: mailquill
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 8080 }
      health_checks:
        - timeout: 5s
          interval: 30s
          unhealthy_threshold: 3
          healthy_threshold: 1
          http_health_check:
            path: /api/health
```

## Verifying the Setup

```sh
# Health through the proxy
curl https://mail.example.com/api/health
# → {"status":"ok"}

# SSE stream: should connect and stay open, printing keep-alive events.
# If it returns immediately or events arrive in bursts, buffering is on.
curl -N https://mail.example.com/api/events?token=<jwt>
```

After the proxy is in place, update `.env`:

```sh
APP_BASE_URL=https://mail.example.com
SERVER_HOST=127.0.0.1
```

If Google/Microsoft OAuth is configured, update the authorized redirect URIs
to the new public URL (see [oauth-gmail-outlook.md](oauth-gmail-outlook.md)).
