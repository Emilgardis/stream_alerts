group "default" {
    targets = ["app"]
}

target "docker-metadata-action" {}

target "app" {
    inherits = ["docker-metadata-action"]
    platforms = ["linux/amd64"]
    args = {
        BUILD_DEPS="openssl git wget libssl-dev pkg-config build-essential curl"
        RUN_DEPS="ca-certificates openssl"
    }
}