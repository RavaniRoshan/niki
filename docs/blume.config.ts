import { defineConfig } from "blume";

export default defineConfig({
  title: "NIKI",
  description: "Turn a sentence into a verified pull request. Hermetic multi-agent coding engine.",
  logo: {
    image: "/logo.svg",
    text: "NIKI",
  },
  github: {
    owner: "RavaniRoshan",
    repo: "niki",
    branch: "master",
    dir: "docs",
  },
  theme: {
    accent: {
      dark: "#4ecdc4", // Brand Teal (hero / success) from token.md
      light: "#0d9488", // Darkened for light background
    },
    action: "#5d8fd6", // Brand Blue (links, info, interactive) from token.md
    background: {
      dark: "#010409", // BG_DEEP_DARK from token.md
      light: "#f8fafc", // BG_BASE_LIGHT from token.md
    },
    mode: "dark",
    radius: "md",
  },
  content: {
    root: "content",
  },
  deployment: {
    site: "https://ravaniroshan.github.io",
    base: "/niki/",
    output: "static",
  },
  ai: {
    llmsTxt: true,
    webmcp: true,
  },
  markdown: {
    headingAnchors: true,
    imageZoom: true,
    codeBlocks: {
      theme: {
        dark: "github-dark",
        light: "github-light",
      },
    },
  },
});
