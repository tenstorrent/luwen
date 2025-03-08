#!/bin/bash

cargo release --workspace `comm -23 <(cargo ws ls --json | jq -r '.[].name' | sort) <(cargo ws changed --json | jq -r '.[].name' | sort) | xargs -I {} echo --exclude {}` --no-publish --no-push $2 $1 
