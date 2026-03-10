class Curd < Formula
  desc "Semantic Codebase Understanding and Refactoring Engine"
  homepage "https://github.com/bharath/CURD" # Replace with actual URL
  url "https://github.com/bharath/CURD/releases/download/v0.7.0-beta/curd-macos.tar.gz" # Placeholder
  sha256 "REPLACE_WITH_ACTUAL_SHA256" # Placeholder
  license "MIT"
  version "0.7.0-beta"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--workspace"
    bin.install "target/release/curd"
  end

  test do
    system "#{bin}/curd", "--version"
  end
end
