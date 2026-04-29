FROM node:22-alpine AS builder
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm install
COPY . .
# VITE environment variables injected at build time via --build-arg
ARG VITE_API_BASE_URL=/api
ARG VITE_MEMORYOPS_WORKSPACE_ID
ENV VITE_API_BASE_URL=$VITE_API_BASE_URL
ENV VITE_MEMORYOPS_WORKSPACE_ID=$VITE_MEMORYOPS_WORKSPACE_ID
RUN npm run build

FROM nginx:1.27-alpine
COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
