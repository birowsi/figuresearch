# FigureSearch web app: React frontend + Rust search server in one image.

FROM node:22-bookworm-slim AS web
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY index.html vite.config.ts tsconfig.json sites.json ./
COPY src ./src
RUN npm run build

FROM rust:1-bookworm AS server
WORKDIR /app
COPY sites.json ./
COPY src-tauri ./src-tauri
RUN cargo build --release --locked -p figuresearch-server --manifest-path src-tauri/Cargo.toml

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=server /app/src-tauri/target/release/figuresearch-server ./
COPY --from=web /app/dist ./dist
ENV DIST_DIR=/app/dist PORT=10000
EXPOSE 10000
CMD ["./figuresearch-server"]
