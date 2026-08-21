#!/usr/bin/env bash

# Run from the project root
cd $(dirname ${BASH_SOURCE[0]})/../..

docker run --rm -it \
    --network=host \
    -v $PWD:/src \
    -u $(id -u):$(id -g) \
    -w /src \
    -e HOME=/src/tmp/mounted-home \
    --name agents-test \
    agents-test