class Niki < Formula
  desc "Hermetic multi-agent coding system"
  homepage "https://github.com/RavaniRoshan/niki"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.3.1/niki-aarch64-apple-darwin.tar.xz"
      sha256 "cbe3e56070ed9d245f9b25ec851a5e2c86cf7a2ff522c9c2dd8d10fe9cb0bc58"
    end
    on_intel do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.3.1/niki-x86_64-apple-darwin.tar.xz"
      sha256 "36d281d412e25c836e842cf5293c01237d7f0f5ae239e2e55abd29b161bf5b88"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.3.1/niki-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "277afd6d8b90254514ff69479c2b3fb163f8a5cc7c255ccce62a65b9c0b0b19e"
    end
    on_intel do
      url "https://github.com/RavaniRoshan/niki/releases/download/v0.3.1/niki-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "c1e2e4c6cc24e2d0f381f83aafb55b9dfde13c4b050ca7cee8d68f44124a09ca"
    end
  end

  def install
    bin.install "niki"
  end

  test do
    assert_match "niki #{version}", shell_output("#{bin}/niki --version")
  end
end
