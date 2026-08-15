class Niki < Formula
  desc "Hermetic multi-agent coding system"
  homepage "https://github.com/RavaniRoshan/niki"
  license "Apache-2.0"
  version "0.4.0"

  # SHA256 values are filled from the GitHub release assets at launch time:
  #   gh release download v0.4.0 -D /tmp/niki-rel && cd /tmp/niki-rel && sha256sum *.tar.gz
  on_macos do
    on_arm do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.4.0/niki-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.4.0/niki-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.4.0/niki-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "niki"
  end

  test do
    assert_match "niki #{version}", shell_output("#{bin}/niki --version")
  end
end
