#!/bin/bash

set -euxo pipefail

if [[ -z "$ANDROID_HOME" ]] then 
    echo ANDROID_HOME is not set!
    exit 1
fi

java-spaghetti-gen generate
rustfmt bindings.rs --edition 2021
