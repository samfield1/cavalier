FROM ghcr.io/samfield1/cavalier-trunk:latest AS builder

WORKDIR /docker
COPY . .
ENV PATH="/root/.cargo/bin:${PATH}"
RUN echo $PATH
RUN trunk build --release

FROM nginx:latest AS final

COPY nginx.conf /etc/nginx/nginx.conf
COPY --from=builder /docker/dist/ /usr/share/nginx/html/
