#!/bin/bash

SCRIPTPATH="$( cd "$(dirname "$0")" ; pwd -P )"
cd $SCRIPTPATH

cp src/back_lib.rs src/lib.rs
cargo build


