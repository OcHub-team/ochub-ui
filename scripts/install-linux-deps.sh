#!/usr/bin/env bash
set -euo pipefail

sudo apt-get update
sudo apt-get install --no-install-recommends -y \
    build-essential \
    clang \
    cmake \
    libasound2-dev \
    libfontconfig-dev \
    libglib2.0-dev \
    libssl-dev \
    libvulkan1 \
    libwayland-dev \
    libx11-xcb-dev \
    libxkbcommon-x11-dev \
    pkg-config
