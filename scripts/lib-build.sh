source .env
docker build --target=bin-release --output type=tar,dest=lib.tar  --build-arg REDIS_VERSION=$REDIS_VERSION ./redis_hooks
tar -xf lib.tar --one-top-level=release
rm ./lib.tar