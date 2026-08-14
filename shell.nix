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

    # Database & Security
    sqlite
    openssl

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

    echo "🚀 Nexus Suite NixOS Development Shell Activated!"
    echo "   Graphics, Wayland, Fontconfig & SQLite drivers configured."
  '';
}
