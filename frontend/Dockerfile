FROM node:26-alpine AS builder
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --legacy-peer-deps
COPY . .
ARG VITE_API_BASE_URL=/api
ARG VITE_MEMORYOPS_WORKSPACE_ID
ENV VITE_API_BASE_URL=$VITE_API_BASE_URL
ENV VITE_MEMORYOPS_WORKSPACE_ID=$VITE_MEMORYOPS_WORKSPACE_ID
RUN npx tsc -b && npx vite build

FROM rust:1.98-alpine AS server-builder
WORKDIR /src
COPY server.rs .
RUN rustc -C target-feature=+crt-static server.rs -O -o /memoryops-frontend

FROM scratch
COPY --from=builder /app/dist /usr/share/memoryops/html
COPY --from=server-builder /memoryops-frontend /memoryops-frontend
ENV MEMORYOPS_STATIC_ROOT=/usr/share/memoryops/html
ENV MEMORYOPS_FRONTEND_BIND=0.0.0.0:8080
ENV MEMORYOPS_API_UPSTREAM=api:8080
USER 65532:65532
EXPOSE 8080
CMD ["/memoryops-frontend"]
