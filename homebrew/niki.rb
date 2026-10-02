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
      sha256 "4ac49685f174d4010a73502d53a593ef15c1ecf393abd04dab886a3ce19c3d65"
    end
    on_intel do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.9.0/niki-x86_64-apple-darwin.tar.xz"
      sha256 "c4aecdcda7a9abcd3496eb36b104b5dcb7fc740d8e00a18a6d817d094568b88e"
    end
  end

  on_linux do
    if OS.mac? || Hardware::CPU.intel?
      on_intel do
        url "https://github.com/RavaniRoshan/niki/releases/download/v0.9.0/niki-x86_64-unknown-linux-gnu.tar.xz"
        sha256 "160b707373ba90ffd9069afcc121ef7e2972d39e49d4322f29c4bbcf038c6768"
      end
    end
    on_arm do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.9.0/niki-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "7cd8519d841ff818368b11178f7b7bd345b9ca146b101ac92a68902a65b4e068"
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
