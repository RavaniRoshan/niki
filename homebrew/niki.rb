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
      sha256 "b4a1bcd062ee409796909894a88f7118e8a5440288b10b70dad4b1f83fcbaa8d"
    end
    on_intel do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.4.0/niki-x86_64-apple-darwin.tar.gz"
      sha256 "b10b8695b3184979d6289fc3165d0d3b8247f67ce0ada5c3e03a8f6e01cae2f7"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.4.0/niki-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e54ba2aa065ab00bfd4b33e25c9512c8f9b833dd139ff1f8eda4f390743782e4"
    end
  end

  def install
    bin.install "niki"
  end

  test do
    assert_match "niki #{version}", shell_output("#{bin}/niki --version")
  end
end
