// @ts-check
import { defineConfig } from "astro/config";
import vue from "@astrojs/vue";

// https://astro.build/config
export default defineConfig({
  site: "https://portside-labs.github.io",
  base: "/rendrr",
  integrations: [vue()],
  markdown: {
    shikiConfig: {
      theme: "night-owl",
      wrap: true,
    },
  },
});
