{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [ pkg-config cmake clang llvmPackages.libclang rustc cargo ];
  buildInputs = with pkgs; [
    libopus
    alsa-lib
    webrtc-audio-processing
    pipewire
    abseil-cpp
  ];
  shellHook = ''
    export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
    export PKG_CONFIG_PATH="${pkgs.libopus.dev}/lib/pkgconfig:${pkgs.alsa-lib.dev}/lib/pkgconfig:${pkgs.webrtc-audio-processing}/lib/pkgconfig:$PKG_CONFIG_PATH"
  '';
}
