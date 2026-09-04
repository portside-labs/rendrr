/**
 * Client-side Shiki highlighter for the API Reference page.
 *
 * Uses fully static imports of languages + theme + engine so there are no
 * runtime dynamic imports for Vite to chunk (which was failing silently in
 * the Vue island). The whole highlighter is loaded eagerly the first time
 * a `<CodeBlock>` mounts.
 */
import { createHighlighterCore, type HighlighterCore } from "shiki/core";
import { createOnigurumaEngine } from "shiki/engine/oniguruma";

import bash from "shiki/langs/bash.mjs";
import javascript from "shiki/langs/javascript.mjs";
import php from "shiki/langs/php.mjs";
import python from "shiki/langs/python.mjs";
import java from "shiki/langs/java.mjs";
import ruby from "shiki/langs/ruby.mjs";
import go from "shiki/langs/go.mjs";
import json from "shiki/langs/json.mjs";

import nightOwl from "shiki/themes/night-owl.mjs";

const THEME = "night-owl";

let _highlighter: HighlighterCore | null = null;
let _promise: Promise<HighlighterCore> | null = null;

function getHighlighter(): Promise<HighlighterCore> {
  if (_highlighter) return Promise.resolve(_highlighter);
  if (_promise) return _promise;
  _promise = createHighlighterCore({
    themes: [nightOwl],
    langs: [bash, javascript, php, python, java, ruby, go, json],
    engine: createOnigurumaEngine(() => import("shiki/wasm")),
  }).then((h) => {
    _highlighter = h;
    return h;
  });
  return _promise;
}

export async function highlight(code: string, lang: string): Promise<string> {
  try {
    const h = await getHighlighter();
    const loaded = h.getLoadedLanguages();
    const safeLang = loaded.includes(lang as any) ? lang : "text";
    return h.codeToHtml(code, { lang: safeLang, theme: THEME });
  } catch (err) {
    console.error("[Shiki] highlight failed:", err);
    return "";
  }
}
