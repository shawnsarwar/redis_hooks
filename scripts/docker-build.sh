source .env
docker build --tag shawnsarwar/redis_hooks:local --target=redis --build-arg REDIS_VERSION=$REDIS_VERSION ./redis_hooks
