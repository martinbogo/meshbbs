# Meshtastic Protobuf Files

This directory is used to store the Meshtastic protobuf definition files when building with the `meshtastic-proto` feature.

## Usage

The actual Meshtastic protobuf definitions are maintained upstream at:
https://github.com/meshtastic/protobufs

To use the real definitions:

1. Clone the upstream repository (or add as a git submodule):
```bash
git clone https://github.com/meshtastic/protobufs.git third_party/meshtastic-protobufs
```

2. Point the build to those protos during build:
```bash
MESHTASTIC_PROTO_DIR=third_party/meshtastic-protobufs/src proto cargo build --features meshtastic-proto
```

Alternatively, copy the required `.proto` files into this directory.

## Placeholder

A placeholder proto file will be generated automatically if no `.proto` files are found, so that the build succeeds. Replace it with the real protos for full functionality.
