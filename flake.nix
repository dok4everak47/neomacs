{
  description = "neomacs — a GPU-accelerated Emacs written in Rust (local dev shell)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    # rust-overlay: 提供锁定的最新 stable toolchain + rust-src
    # (模板注释建议的 per-project toolchain 方案)
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
  }: let
    # 本机 Apple Silicon macOS；日后如需跨平台再扩展 systems
    system = "aarch64-darwin";
    pkgs = import nixpkgs {
      inherit system;
      overlays = [rust-overlay.overlays.default];
    };
    # 锁定到最新 stable + 开发扩展
    rustToolchain = pkgs.rust-bin.stable.latest.default.override {
      extensions = ["rust-src" "rust-analyzer" "clippy" "rustfmt"];
    };
  in {
    devShells.${system}.default = pkgs.mkShell {
      packages = [
        rustToolchain
        # neomacs-terminfo's build.rs probes pkg-config for ncurses/ncursesw.
        # The Nix apple-sdk ships no libncurses.tbd, so both the .pc files
        # (pkg-config) and the linkable library (ncurses) must come from
        # nixpkgs; without them the final link fails with
        # "ld: library not found for -lncurses".
        pkgs.pkg-config
        pkgs.ncurses
      ];
      RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

      shellHook = ''
        # ClashBar proxy on 127.0.0.1:7890 -- exported only while it is
        # listening, so a dead proxy never breaks the shell's network.
        if (echo > /dev/tcp/127.0.0.1/7890) 2>/dev/null; then
          export http_proxy=http://127.0.0.1:7890 https_proxy=http://127.0.0.1:7890
          export HTTP_PROXY=http://127.0.0.1:7890 HTTPS_PROXY=http://127.0.0.1:7890
          export all_proxy=socks5://127.0.0.1:7890 ALL_PROXY=socks5://127.0.0.1:7890
          export no_proxy=localhost,127.0.0.1,::1 NO_PROXY=localhost,127.0.0.1,::1
        fi
      '';
    };
  };
}
