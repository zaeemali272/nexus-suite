{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  name = "nexus-suite-dev-shell";

  buildInputs = with pkgs; [
    # System & Rust Build Tools
    cargo
    rustc
    pkg-config
    cmake
    gcc

    # Database, Security & P2P Mesh Networking
    sqlite
    openssl
    tailscale

    # UI Renderer & Graphics (Slint, Fontconfig, Wayland, X11, OpenGL, Vulkan)
    fontconfig
    freetype
    libxkbcommon
    wayland
    libGL
    libglvnd
    vulkan-loader

    # Modern X11 Windowing Libraries
    libx11
    libxcursor
    libxrandr
    libxi
    libxrender
    libxcb
  ];

  shellHook = ''
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath (with pkgs; [
      libGL
      libglvnd
      vulkan-loader
      wayland
      libxkbcommon
      fontconfig
      freetype
      libx11
      libxcursor
      libxrandr
      libxi
      libxrender
      libxcb
    ])}:$LD_LIBRARY_PATH"

    export PKG_CONFIG_PATH="${pkgs.fontconfig.dev}/lib/pkgconfig:${pkgs.sqlite.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
    export RUST_BACKTRACE=1

    # Developer Shortcuts & Helper Aliases
    alias nexus-client="cargo run -p nexus-client"
    alias nexus-daemon="cargo run -p nexus-daemon"
    alias nexus-test="cargo test --workspace"
    alias nexus-check="cargo check --workspace"
    alias nexus-tailscale="tailscale status"

    echo "=========================================================================="
    echo "🚀 Nexus Suite NixOS Development Shell Activated!"
    echo "   Graphics, Wayland, Fontconfig & SQLite drivers loaded."
    echo "=========================================================================="

    # Tailscale P2P Mesh IP Detection
    if command -v tailscale >/dev/null 2>&1; then
      TS_IP=$(tailscale ip -4 2>/dev/null || echo "")
      if [ -n "$TS_IP" ]; then
        echo "🌐 Tailscale Mesh IPv4 : $TS_IP"
        echo "   (Use this IP for direct P2P mesh testing with peer devices)"
      else
        echo "⚠️ Tailscale is installed but not connected."
        echo "   Run: 'sudo systemctl start tailscaled' and 'sudo tailscale up'"
      fi
    fi

    echo ""
    echo "💡 Shortcuts Available:"
    echo "   • nexus-client     : Build & run Slint Native GPU UI"
    echo "   • nexus-daemon     : Build & run Headless Tokio Daemon"
    echo "   • nexus-test       : Run all workspace unit tests"
    echo "   • nexus-check      : Verify workspace (#![deny(warnings)])"
    echo "   • nexus-tailscale  : View Tailscale P2P mesh status"
    echo "=========================================================================="
  '';
}
