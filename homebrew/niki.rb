class Niki < Formula
  desc "Hermetic multi-agent coding system"
  homepage "https://github.com/RavaniRoshan/niki"
  license "Apache-2.0"
  version "0.9.0"

  # SHA256 values come from the release's sha256.sum:
  #   curl -fsSL -O https://github.com/RavaniRoshan/niki/releases/download/v0.9.0/sha256.sum
  # Bump `version` and all four URLs+sha256 together when cutting a release.
  on_macos do
    on_arm do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.9.0/niki-aarch64-apple-darwin.tar.xz"
      sha256 "47f2d897ef26bd8c6d9d8c0acd2cb40ecc50e14d08ee8b6ce482edba92b00b25"
    end
    on_intel do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.9.0/niki-x86_64-apple-darwin.tar.xz"
      sha256 "21e0835c794d9743ff3640fad5b45c90acdf1a1eedaf3ffc76a0cad91540a170"
    end
  end

  on_linux do
    if OS.mac? || Hardware::CPU.intel?
      on_intel do
        url "https://github.com/RavaniRoshan/niki/releases/download/v0.9.0/niki-x86_64-unknown-linux-gnu.tar.xz"
        sha256 "7ac38dbf239e4ea369ef82852b08ffb9a0dc1046dab7f6fb62ad1ae323bfe28c"
      end
    end
    on_arm do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.9.0/niki-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "0f030ad9fadc393eea06e80017e3126756e1a6b2e3791841de79a673e7a7b352"
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
