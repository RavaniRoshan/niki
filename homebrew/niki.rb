class Niki < Formula
  desc "Hermetic multi-agent coding system"
  homepage "https://github.com/RavaniRoshan/niki"
  license "Apache-2.0"
  version "0.7.0"

  # SHA256 values are filled from the GitHub release assets at launch time:
  #   gh release download v0.7.0 -p '*.sha256' -D /tmp/niki-rel
  on_macos do
    on_arm do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.7.0/niki-aarch64-apple-darwin.tar.xz"
      sha256 "40d3fcb8c34c903a4dd26c3f0c067e05a10d33643305030c7cbd47d993a7d6f5"
    end
    on_intel do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.7.0/niki-x86_64-apple-darwin.tar.xz"
      sha256 "e6b78acfd4c75110645b941185726a0b7465ac2b4788ce72ebc4bc7cdebddb1f"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.7.0/niki-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "179a1f620a3c487b8646b0be1c4b9657e9226125d23d9cf9a9224a412d214fc5"
    end
  end

  def install
    bin.install "niki"
  end

  test do
    assert_match "niki #{version}", shell_output("#{bin}/niki --version")
  end
end
