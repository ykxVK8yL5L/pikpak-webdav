FROM alpine:latest
ARG TARGETARCH
ARG TARGETVARIANT
RUN apk --no-cache add ca-certificates tini
RUN apk add tzdata && \
	cp /usr/share/zoneinfo/Asia/Shanghai /etc/localtime && \
	echo "Asia/Shanghai" > /etc/timezone && \
	apk del tzdata

RUN mkdir -p /etc/pikpak-webdav
WORKDIR /root/
ADD pikpak-webdav-$TARGETARCH$TARGETVARIANT /usr/bin/pikpak-webdav

ENTRYPOINT ["/sbin/tini", "--"]
CMD ["/usr/bin/pikpak-webdav", "--host", "0.0.0.0", "--workdir", "/etc/pikpak-webdav"]
