class Niki < Formula
  desc "Hermetic multi-agent coding system"
  homepage "https://github.com/RavaniRoshan/niki"
  license "Apache-2.0"
  version "0.10.0"

  # SHA256 values come from the release's sha256.sum:
  #   curl -fsSL -O https://github.com/RavaniRoshan/niki/releases/download/v0.10.0/sha256.sum
  # Bump `version` and all four URLs+sha256 together when cutting a release.
  on_macos do
    on_arm do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.10.0/niki-aarch64-apple-darwin.tar.xz"
      sha256 "96981ad43e32f134d170bb03aa253493fa97f5655e613054d3633ad2a156e0d0"
    end
    on_intel do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.10.0/niki-x86_64-apple-darwin.tar.xz"
      sha256 "5183602807841aa6f7b18cf3cd7e16cd45caa00bff8c84627208f4051063365f"
    end
  end

  on_linux do
    if OS.mac? || Hardware::CPU.intel?
      on_intel do
        url "https://github.com/RavaniRoshan/niki/releases/download/v0.10.0/niki-x86_64-unknown-linux-gnu.tar.xz"
        sha256 "ed5aa0e436ee47e700f07c50818b60ced43b7cfc1a3aff38661e5ec60a3468d9"
      end
    end
    on_arm do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.10.0/niki-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "de18acac3640747e54e59365bcfb5bf3bcb976d9799812f24702d5bfe43ea0ea"
    end
  end

  def install
    # cargo-dist archives carry a top-level directory named after the target
    # triple, so the binary is not at the archive root.
    bin.install Dir["niki-*/niki"].first => "niki"
  end

  test do
    assert_match "niki #{version}", shell_output("#{bin}/niki --version")
  end
end
