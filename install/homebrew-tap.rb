# Homebrew tap formula for garagetytus.
#
# Lives in github.com/traylinx/homebrew-tap/Formula/garagetytus.rb
# once the tap repo exists; this file is the canonical source.
#
# Per LD#7 amendment (Q1 verdict 2026-04-25) — Mac path uses
# Homebrew compile-from-source via `depends_on "garage"`. We do
# NOT vendor or rebuild Garage; the upstream `garage` formula
# already pins the AGPL source tarball SHA at its own
# `sha256` declaration, which is the source-availability artifact.

class Garagetytus < Formula
  desc "Local S3 daemon for every dev laptop, powered by Garage"
  homepage "https://garagetytus.dev"
  license "MIT"

  # Replace these two with the actual cargo-dist release artifacts
  # at release-cut time.
  version "0.1.0"

  on_macos do
    on_arm do
      url "https://github.com/traylinx/garagetytus/releases/download/v#{version}/garagetytus-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_AT_RELEASE_TIME"
    end
    on_intel do
      url "https://github.com/traylinx/garagetytus/releases/download/v#{version}/garagetytus-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_AT_RELEASE_TIME"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/traylinx/garagetytus/releases/download/v#{version}/garagetytus-x86_64-unknown-linux-musl.tar.gz"
      sha256 "REPLACE_AT_RELEASE_TIME"
    end
    on_arm do
      url "https://github.com/traylinx/garagetytus/releases/download/v#{version}/garagetytus-aarch64-unknown-linux-musl.tar.gz"
      sha256 "REPLACE_AT_RELEASE_TIME"
    end
  end

  # Garage stays a child process — never linked. The upstream
  # `garage` formula compiles the AGPL source tarball; that's the
  # Mac source-availability path per LD#7.
  depends_on "garage"

  def install
    bin.install "garagetytus"
  end

  test do
    assert_match "garagetytus", shell_output("#{bin}/garagetytus --version")
  end
end
