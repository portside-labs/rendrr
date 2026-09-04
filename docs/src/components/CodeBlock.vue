<script setup lang="ts">
import { ref, watch } from "vue";
import { highlight as runHighlight } from "~/lib/highlighter";

interface Props {
  code: string;
  language?: string;
  title?: string;
  copyable?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  language: "text",
  copyable: true,
});

const copied = ref(false);
const highlightedHtml = ref("");

async function doHighlight() {
  highlightedHtml.value = await runHighlight(props.code, props.language);
}

watch(() => [props.code, props.language], doHighlight, { immediate: true });

async function copy() {
  await navigator.clipboard.writeText(props.code);
  copied.value = true;
  setTimeout(() => {
    copied.value = false;
  }, 2000);
}
</script>

<template>
  <div class="r-codeblock">
    <div class="r-codeblock__header" v-if="title || language">
      <span class="r-codeblock__title">{{ title || language }}</span>
      <div class="r-codeblock__actions">
        <slot name="header-actions" />
        <button v-if="copyable" class="r-codeblock__copy" @click="copy">
          {{ copied ? "Copied" : "Copy" }}
        </button>
      </div>
    </div>
    <div v-if="highlightedHtml" class="r-codeblock__highlighted" v-html="highlightedHtml" />
    <pre v-else class="r-codeblock__pre"><code>{{ code }}</code></pre>
  </div>
</template>

<style scoped>
.r-codeblock {
  border-radius: var(--radius-lg);
  overflow: hidden;
  border: 1px solid rgba(255, 255, 255, 0.04);
  margin-bottom: var(--space-6);
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.15);
}

.r-codeblock__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-3) var(--space-5);
  background: var(--code-surface);
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.r-codeblock__title {
  font-family: var(--font-ui);
  font-size: 0.6875rem;
  font-weight: 500;
  color: #7d8b9d;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.r-codeblock__actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.r-codeblock__copy {
  font-family: var(--font-ui);
  font-size: 0.6875rem;
  font-weight: 500;
  color: #7d8b9d;
  background: none;
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: var(--radius-sm);
  padding: 3px var(--space-3);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.r-codeblock__copy:hover {
  color: #b4c1d1;
  border-color: rgba(255, 255, 255, 0.15);
  background: rgba(255, 255, 255, 0.04);
}

.r-codeblock__pre {
  margin: 0;
  border-radius: 0;
  background: var(--code-bg);
  color: #c9d1d9;
  padding: var(--space-6);
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  line-height: 1.7;
}

.r-codeblock__highlighted :deep(pre) {
  margin: 0;
  border-radius: 0;
  background: var(--code-bg) !important;
  padding: var(--space-6);
  overflow-x: auto;
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  line-height: 1.7;
}

.r-codeblock__highlighted :deep(pre code) {
  background: none;
  border: none;
  padding: 0;
  font-size: inherit;
  color: inherit;
  font-family: inherit;
}

.r-codeblock__highlighted :deep(.line) {
  display: inline;
}
</style>
