# cinebox-tmdb-proxy

A caching proxy for the [TMDB](https://www.themoviedb.org/) API and images. Put it on a server that can reach TMDB, give it a domain, and point [Cinebox](https://github.com/dexsper/cinebox) at that domain.

Why: in Russia and a few other places TMDB is blocked. Cinebox gets past a DNS block on its own with DNS-over-HTTPS, but not past a block by IP. That is what the proxy is for.

- `/3/...` goes to the TMDB API and `/t/p/{size}/{file}` to TMDB images. TMDB links in responses are replaced with the proxy's address, so clients never touch TMDB hosts.
- One cache for all users, in memory and on disk (5 GiB by default). While TMDB is down it serves what it has, up to 30 days old.
- One static binary. It runs on a Raspberry Pi with 512 MB of RAM and uses every core on a big server.
- Only TMDB paths are allowed, so it is not an open proxy.

The proxy speaks plain HTTP (HTTP/1.1 and h2c). HTTPS is up to whatever you put in front of it: nginx, Caddy, Traefik, anything. `/info` tells Cinebox that this is a proxy and whether it needs a key, `/healthz` is for monitoring.

## Binary

Archives are on the [Releases](https://github.com/dexsper/cinebox/releases) page:

| Archive | For |
| --- | --- |
| `cinebox-tmdb-proxy-<version>-windows-x64.zip` | Windows x64 |
| `cinebox-tmdb-proxy-<version>-linux-x86_64.tar.gz` | Linux x86_64 |
| `cinebox-tmdb-proxy-<version>-linux-aarch64.tar.gz` | 64-bit ARM: Raspberry Pi 3/4/5 with a 64-bit OS, ARM servers |
| `cinebox-tmdb-proxy-<version>-linux-armv7.tar.gz` | 32-bit ARM: 32-bit Raspberry Pi OS, routers, TV boxes |

The Linux binaries are static and need no libraries. Each archive has a `.sha256` next to it.

```bash
tar -xzf cinebox-tmdb-proxy-*-linux-x86_64.tar.gz
cd cinebox-tmdb-proxy-*-linux-x86_64
./cinebox-tmdb-proxy --public-url https://tmdb.example.com
```

On Windows it is the same with `cinebox-tmdb-proxy.exe`. By default the proxy listens on `127.0.0.1:8080` and keeps its cache in `./cache`. `--public-url` is the address users reach it by; TMDB links in responses are rewritten to it.

To check it:

```bash
curl http://127.0.0.1:8080/info
curl -s "http://127.0.0.1:8080/3/configuration?api_key=YOUR_KEY" -o /dev/null -D - | grep -i x-cache
```

The first request gives `X-Cache: MISS`, the next one `HIT`.

## systemd

The Linux archives include `cinebox-tmdb-proxy.service`. It runs the proxy as a throwaway user, keeps the cache in `/var/cache/cinebox-tmdb-proxy`, restarts it on failure and locks it down.

```bash
sudo install -m 0755 cinebox-tmdb-proxy /usr/local/bin/
sudo install -m 0644 cinebox-tmdb-proxy.service /etc/systemd/system/
sudo systemctl edit cinebox-tmdb-proxy
```

In the editor:

```ini
[Service]
Environment=PUBLIC_URL=https://tmdb.example.com
Environment=TRUST_FORWARDED_FOR=true
```

`TRUST_FORWARDED_FOR` is needed when a reverse proxy sits in front: without it the per-IP limit sees every user as one client. Turn it on only if the reverse proxy writes the real client address into `X-Forwarded-For` (see below).

Your own TMDB key, if you want one, goes into a file only root can read:

```bash
echo 'TMDB_API_KEY=your-32-character-key' | sudo tee /etc/cinebox-tmdb-proxy.env > /dev/null
sudo chmod 600 /etc/cinebox-tmdb-proxy.env
```

Start it:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now cinebox-tmdb-proxy
journalctl -u cinebox-tmdb-proxy -f
```

## Reverse proxy

Any reverse proxy will do: forward the domain to `127.0.0.1:8080`. The proxy takes the client address from the first value of `X-Forwarded-For`, so the reverse proxy must replace that header, not append to it. Otherwise a client can send any address it likes.

nginx:

```nginx
server {
    listen 443 ssl;
    server_name tmdb.example.com;
    ssl_certificate     /etc/letsencrypt/live/tmdb.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/tmdb.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header X-Forwarded-For $remote_addr;
    }
}
```

Caddy gets the certificate on its own and sets `X-Forwarded-For` correctly by default:

```
tmdb.example.com {
	reverse_proxy 127.0.0.1:8080
}
```

## Docker Compose

`compose.yaml` builds the image from source; nothing is published to a registry. The proxy is reachable on the host at `127.0.0.1:8080`, so you can put your own reverse proxy in front, as above. If you have none, there is a `caddy` profile: Caddy gets the certificate on its own, but the domain must point at the machine and ports 80 and 443 must be open.

```bash
git clone https://github.com/dexsper/cinebox.git
cd cinebox/packaging/tmdb-proxy
cp .env.example .env                           # set DOMAIN, and TMDB_API_KEY if you want
docker compose up -d --build                   # proxy only
docker compose --profile caddy up -d --build   # proxy and Caddy
```

The cache lives in the `cache` volume. The container is capped at 512 MB, and the memory cache takes an eighth of that. To update, run `git pull` and `docker compose up -d --build` again. Other settings go in `environment:` under the variable names from the table below.

## Flags

Any flag can also be set by its environment variable; the flag wins. Sizes are written as `512MiB` or `20GiB`, durations as `6h` or `30d`, booleans as `true`/`false`.

| Flag | Variable | Default | What |
| --- | --- | --- | --- |
| `--listen` | `LISTEN` | `127.0.0.1:8080` | Address to listen on |
| `--public-url` | `PUBLIC_URL` | `http://{listen}` | Address users reach the proxy by; TMDB links are rewritten to it |
| `--cache-dir` | `CACHE_DIR` | `./cache` | Disk cache directory |
| `--disk-cache` | `DISK_CACHE` | `5GiB` | Disk cache size; cut down if the disk has less room |
| `--memory-cache` | `MEMORY_CACHE` | `auto` | Memory cache size; `auto` is 1/8 of RAM (or of the container limit), 16 MiB to 1 GiB |
| `--memory-max-entry` | `MEMORY_MAX_ENTRY` | `64KiB` | Bigger answers are served from disk only |
| `--max-body` | `MAX_BODY` | `32MiB` | Bigger answers are passed on but not cached |
| `--workers` | `WORKERS` | number of cores | Worker threads |
| `--blocking-threads` | `BLOCKING_THREADS` | 4 x cores, at most 64 | Threads for disk I/O |
| `--max-connections` | `MAX_CONNECTIONS` | `auto` | Open connections, 256 per core; over the limit, 503 |
| `--upstream-concurrency` | `UPSTREAM_CONCURRENCY` | `20` | Requests to TMDB at once |
| `--upstream-timeout` | `UPSTREAM_TIMEOUT` | `20s` | Timeout of a request to TMDB |
| `--rate-limit` | `RATE_LIMIT` | `50` | Requests per second from one IP; `0` turns it off |
| `--rate-burst` | `RATE_BURST` | `100` | How far a client may go over the rate for a moment |
| `--trust-forwarded-for` | `TRUST_FORWARDED_FOR` | `false` | Take the client IP from `X-Forwarded-For` |
| `--tmdb-key` | `TMDB_API_KEY` | none | Your TMDB key, see [Keys](#keys) |
| `--ttl-config` | `TTL_CONFIG` | `7d` | How long `/configuration` stays fresh |
| `--ttl-details` | `TTL_DETAILS` | `24h` | Titles, seasons, people, collections |
| `--ttl-search` | `TTL_SEARCH` | `6h` | Search and discover |
| `--ttl-lists` | `TTL_LISTS` | `3h` | Now playing, popular, trending |
| `--ttl-images` | `TTL_IMAGES` | `365d` | Images |
| `--ttl-not-found` | `TTL_NOT_FOUND` | `10m` | 404 answers |
| `--stale-if-error` | `STALE_IF_ERROR` | `30d` | How long an expired answer is still served while TMDB is down |
| `--known-key-ttl` | `KNOWN_KEY_TTL` | `24h` | How long the proxy remembers a key TMDB accepted |
| `--dns-bypass` | `DNS_BYPASS` | `true` | Resolve TMDB over DNS-over-HTTPS if the system DNS fails |
| `--doh-url` | `DOH_URL` | empty | Your own DoH server; empty means the built-in list |
| `--system-proxy` | `SYSTEM_PROXY` | `false` | Reach TMDB through `HTTPS_PROXY` and the like |
| `--stats-interval` | `STATS_INTERVAL` | `10m` | How often the stats line goes to the log |

The log level is set by `RUST_LOG`, `info` by default. `--help` lists every flag.

## Keys

**Users' own keys (default).** Every user sends their own TMDB key, just as Cinebox does without a proxy. A request without a key gets 401. The cache is shared, but a key gets to it only after TMDB has accepted that key once. So the proxy cannot be used as a free TMDB without a key.

**Your key.** With `--tmdb-key` the proxy ignores users' keys and uses yours. Cinebox learns this from `/info` and hides the key field. Every request then counts against your key, so keep the rate limit on for a public server.

## What the proxy sees

Whoever runs a proxy can see what goes through it. This one:

- writes no IPs, paths or keys to the log, only a stats line every 10 minutes;
- keeps user keys only as hashes in memory and forgets them within a day or on restart;
- keeps IPs only in memory, for the rate limit;
- keeps TMDB answers in the disk cache: you can tell which titles were asked for, but not who asked;
- shows TMDB the server's IP (and in the default mode the user's key), never the user's IP.

The reverse proxy, the hosting provider and the admin still see connections, and a modified build can log anything. If you do not trust a public server, run your own.

## Cinebox setup

In **Settings > TMDB > Proxy address**, enter the proxy's root, for example `https://tmdb.example.com`, without `/3`. Cinebox adds `/3`, `/t/p` and `/info` itself. The address must be `https://`; plain `http://` works only on the same machine or the local network. If the proxy has its own key, the key field disappears; otherwise enter your key as usual. An empty address means straight to TMDB.

The first-run wizard shows the public proxies from the presets next to the official TMDB. To list yours there, add it to [`presets/services.json`](../../presets/services.json) (`https://` only):

```json
"tmdb": [
  { "url": "https://tmdb.example.com" }
]
```
