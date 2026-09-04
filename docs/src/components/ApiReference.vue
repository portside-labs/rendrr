<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick } from "vue";
import { useOpenApiSpec } from "~/composables/useOpenApiSpec";
import { useCodeLanguage } from "~/composables/useCodeLanguage";
import CodeBlock from "./CodeBlock.vue";
import LangPicker from "./LangPicker.vue";

const { endpoints, tags, loading, error } = useOpenApiSpec();
const { selected: selectedLang, current: currentLang } = useCodeLanguage();
const openDropdownId = ref<string | null>(null);

function toggleLangDropdown(epId: string) {
  openDropdownId.value = openDropdownId.value === epId ? null : epId;
}

function selectLang(id: string) {
  const { setLanguage } = useCodeLanguage();
  setLanguage(id as any);
  openDropdownId.value = null;
}

function onClickOutsideLangDropdown(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (!target.closest(".lang-picker")) {
    openDropdownId.value = null;
  }
}

onMounted(() => {
  document.addEventListener("click", onClickOutsideLangDropdown);
});

onUnmounted(() => {
  document.removeEventListener("click", onClickOutsideLangDropdown);
});

const activeId = ref("");
const activeResponseTab = ref<Record<string, string>>({});

function getActiveResponseTab(epId: string, responses: any[]) {
  if (!activeResponseTab.value[epId]) {
    const first = responses.find((r: any) => r.status.startsWith("2"));
    activeResponseTab.value[epId] = first?.status || responses[0]?.status || "";
  }
  return activeResponseTab.value[epId];
}

function setActiveResponseTab(epId: string, status: string) {
  activeResponseTab.value[epId] = status;
}

function getActiveResponseExample(ep: any) {
  const activeStatus = getActiveResponseTab(ep.id, ep.responses);
  const resp = ep.responses.find((r: any) => r.status === activeStatus);
  if (!resp) return "";
  if (resp.examples.length === 0) return "No content";
  return JSON.stringify(resp.examples[0].value, null, 2);
}

onMounted(() => {
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          activeId.value = entry.target.id;
        }
      }
    },
    { rootMargin: "-80px 0px -60% 0px" }
  );

  watch(
    endpoints,
    () => {
      nextTick(() => {
        document.querySelectorAll(".endpoint-section").forEach((el) => {
          observer.observe(el);
        });
      });
    },
    { immediate: true }
  );
});

function scrollTo(id: string) {
  const el = document.getElementById(id);
  if (el) el.scrollIntoView({ behavior: "smooth", block: "start" });
}

const methodColors: Record<string, string> = {
  GET: "var(--method-get)",
  POST: "var(--method-post)",
  PUT: "var(--method-put)",
  DELETE: "var(--method-delete)",
};

function methodColor(method: string) {
  return methodColors[method] || "var(--ink-muted)";
}

// Compact 3-4 letter abbreviations so every badge fits the same width.
const methodLabels: Record<string, string> = {
  GET: "GET",
  POST: "POST",
  PUT: "PUT",
  PATCH: "PAT",
  DELETE: "DEL",
};

function methodLabel(method: string) {
  return methodLabels[method] || method.slice(0, 4);
}

const base = import.meta.env.BASE_URL.replace(/\/$/, "");
</script>

<template>
  <div class="api-ref">
    <div v-if="loading" class="api-ref-loading">Loading API reference...</div>
    <div v-else-if="error" class="api-ref-error">{{ error }}</div>
    <template v-else>
      <aside class="api-ref-nav">
        <a :href="`${base}/`" class="api-ref-back">&larr; Docs</a>
        <h5 class="api-ref-nav-title">API Reference</h5>
        <div v-for="tag in tags" :key="tag" class="api-ref-nav-group">
          <div class="api-ref-nav-tag">{{ tag }}</div>
          <button
            v-for="ep in endpoints.filter((e) => e.tag === tag)"
            :key="ep.id"
            class="api-ref-nav-link"
            :class="{ 'api-ref-nav-link--active': activeId === ep.id }"
            @click="scrollTo(ep.id)"
          >
            <span class="nav-link-label">{{ ep.summary }}</span>
            <span class="nav-method-badge" :style="{ background: methodColor(ep.method) }">
              {{ methodLabel(ep.method) }}
            </span>
          </button>
        </div>
      </aside>

      <div class="api-ref-content">
        <div class="api-ref-header">
          <h1>API Reference</h1>
          <p class="api-ref-auth-note">
            Rendrr runs unauthenticated by default — endpoints accept requests with no
            <code>Authorization</code> header. When OAuth is enabled
            (see <a :href="`${base}/getting-started`">Getting started</a>), every request must
            include <code>Authorization: Bearer &lt;jwt&gt;</code> issued by your IdP,
            carrying the scope listed under each endpoint below. The scope separator
            is configurable via <code>RENDRR_SCOPE_SEPARATOR</code>.
          </p>
        </div>

        <div v-for="ep in endpoints" :key="ep.id" :id="ep.id" class="endpoint-section">
          <div class="endpoint-docs">
            <div class="endpoint-title">
              <span class="method-badge" :style="{ background: methodColor(ep.method) }">
                {{ ep.method }}
              </span>
              <code class="endpoint-path">{{ ep.path }}</code>
            </div>

            <h2 class="endpoint-summary">{{ ep.summary }}</h2>
            <p class="endpoint-desc">{{ ep.description }}</p>

            <div v-if="ep.requiredScope" class="endpoint-block">
              <h3>Authorization</h3>
              <div class="endpoint-auth-row">
                <span class="endpoint-auth-label">Required scope</span>
                <code class="endpoint-scope-value">{{ ep.requiredScope }}</code>
              </div>
              <p class="endpoint-auth-footnote">
                when <a :href="`${base}/oauth`">OAuth</a> is enabled
              </p>
            </div>

            <div v-if="ep.parameters.length" class="endpoint-block">
              <h3>Path Parameters</h3>
              <table class="param-table">
                <thead>
                  <tr><th>Name</th><th>Type</th><th>Description</th></tr>
                </thead>
                <tbody>
                  <tr v-for="p in ep.parameters" :key="p.name">
                    <td>
                      <code>{{ p.name }}</code>
                      <span v-if="p.required" class="param-required">required</span>
                    </td>
                    <td><code class="param-type">{{ p.type }}</code></td>
                    <td>
                      {{ p.description }}
                      <div v-if="p.enum && p.enum.length" class="param-enum">
                        <span class="param-enum-label">Allowed values:</span>
                        <code v-for="v in p.enum" :key="v" class="param-enum-value">{{ v }}</code>
                      </div>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>

            <div v-if="ep.requestBody" class="endpoint-block">
              <h3>Request Body</h3>
              <p class="content-type-label"><code>{{ ep.requestBody.contentType }}</code></p>
              <table class="param-table">
                <thead>
                  <tr><th>Field</th><th>Type</th><th>Description</th></tr>
                </thead>
                <tbody>
                  <tr v-for="p in ep.requestBody.properties" :key="p.name">
                    <td>
                      <code>{{ p.name }}</code>
                      <span v-if="p.required" class="param-required">required</span>
                    </td>
                    <td><code class="param-type">{{ p.type }}</code></td>
                    <td>
                      {{ p.description }}
                      <div v-if="p.enum && p.enum.length" class="param-enum">
                        <span class="param-enum-label">Allowed values:</span>
                        <code v-for="v in p.enum" :key="v" class="param-enum-value">{{ v }}</code>
                      </div>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>

            <div class="endpoint-block">
              <h3>Responses</h3>
              <div v-for="r in ep.responses" :key="r.status" class="response-row">
                <span class="response-status" :class="{
                  'response-status--success': r.status.startsWith('2'),
                  'response-status--error': r.status.startsWith('4') || r.status.startsWith('5'),
                }">{{ r.status }}</span>
                <span class="response-desc">{{ r.description }}</span>
              </div>
            </div>
          </div>

          <div class="endpoint-code">
            <div class="code-sticky">
              <CodeBlock
                :key="selectedLang"
                :code="ep.codeExamples[selectedLang]"
                :language="currentLang.highlightLang"
                title="Request"
              >
                <template #header-actions>
                  <LangPicker
                    :ep-id="ep.id"
                    :open-id="openDropdownId"
                    @toggle="toggleLangDropdown"
                    @select="selectLang"
                  />
                </template>
              </CodeBlock>

              <div v-if="ep.responses.length" class="response-tabs-block">
                <div class="response-tabs">
                  <button
                    v-for="r in ep.responses"
                    :key="r.status"
                    class="response-tab"
                    :class="{
                      'response-tab--active': getActiveResponseTab(ep.id, ep.responses) === r.status,
                      'response-tab--success': r.status.startsWith('2'),
                      'response-tab--error': r.status.startsWith('4') || r.status.startsWith('5'),
                    }"
                    @click="setActiveResponseTab(ep.id, r.status)"
                  >{{ r.status }}</button>
                </div>
                <CodeBlock
                  :key="getActiveResponseTab(ep.id, ep.responses)"
                  :code="getActiveResponseExample(ep)"
                  language="json"
                  title="Response"
                />
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.api-ref {
  display: grid;
  grid-template-columns: 260px 1fr;
  min-height: 100vh;
  max-width: 1400px;
  margin: 0 auto;
}

.api-ref-loading,
.api-ref-error {
  grid-column: 1 / -1;
  padding: var(--space-16);
  text-align: center;
  font-family: var(--font-ui);
  color: var(--ink-muted);
}

.api-ref-nav {
  position: sticky;
  top: 80px;
  height: calc(100vh - 80px);
  overflow-y: auto;
  padding: var(--space-6) var(--space-5);
  border-right: 1px solid var(--border-color);
}

.api-ref-back {
  font-family: var(--font-ui);
  font-size: var(--text-xs);
  color: var(--ink-faint);
  text-decoration: none;
  display: block;
  margin-bottom: var(--space-4);
}

.api-ref-back:hover { color: var(--accent); }

.api-ref-nav-title {
  font-family: var(--font-ui);
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--ink-faint);
  margin-bottom: var(--space-5);
}

.api-ref-nav-group { margin-bottom: var(--space-4); }

.api-ref-nav-tag {
  font-family: var(--font-ui);
  font-size: 0.625rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--ink-faint);
  padding: var(--space-1) 0;
  margin-bottom: var(--space-1);
}

.api-ref-nav-link {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  text-align: left;
  background: none;
  border: none;
  font-family: var(--font-ui);
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--ink-muted);
  padding: 6px var(--space-3);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
  justify-content: space-between;
}

.api-ref-nav-link:hover { color: var(--ink); background: var(--paper-warm); }

.api-ref-nav-link--active {
  color: var(--accent);
  background: var(--accent-light);
  font-weight: 600;
}

/* Match the endpoint-header method badge exactly — same font, padding,
   radius, letter-spacing — so the sidebar and content read as one system. */
.nav-method-badge {
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  font-weight: 700;
  color: #fff;
  padding: 3px 8px;
  border-radius: var(--radius-sm);
  letter-spacing: 0.02em;
  line-height: 1.2;
  min-width: 44px;
  text-align: center;
  flex-shrink: 0;
}

.nav-link-label {
  flex: 1;
  min-width: 0;
  line-height: 1.35;
}

.api-ref-content { padding: var(--space-10) var(--space-8); }

.api-ref-header {
  margin-bottom: var(--space-12);
  padding-bottom: var(--space-8);
  border-bottom: 1px solid var(--border-color);
}

.api-ref-header h1 {
  font-size: var(--text-4xl);
  margin-bottom: var(--space-4);
}

.api-ref-auth-note {
  font-family: var(--font-ui);
  font-size: var(--text-sm);
  color: var(--ink-muted);
  line-height: 1.6;
  max-width: 65ch;
}

.endpoint-section {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-8);
  padding: var(--space-10) 0;
  border-bottom: 1px solid var(--border-color);
  scroll-margin-top: 80px;
}

.endpoint-section:last-child { border-bottom: none; }

.endpoint-docs { min-width: 0; }

.endpoint-title {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-bottom: var(--space-3);
}

.method-badge {
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  font-weight: 700;
  color: #fff;
  padding: 3px 8px;
  border-radius: var(--radius-sm);
  letter-spacing: 0.02em;
}

.endpoint-path {
  font-size: var(--text-base);
  font-weight: 500;
  background: none;
  border: none;
  padding: 0;
  color: var(--ink);
}

.endpoint-auth-row {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  font-family: var(--font-ui);
  font-size: var(--text-sm);
}

.endpoint-auth-label {
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--ink-faint);
}

.endpoint-auth-footnote {
  font-family: var(--font-ui);
  font-size: var(--text-xs);
  color: var(--ink-faint);
  font-style: italic;
  margin: var(--space-2) 0 0;
}

.endpoint-scope-value {
  font-family: var(--font-mono);
  font-size: 0.85em;
  font-weight: 500;
  color: var(--method-post);
  background: rgba(63, 108, 149, 0.10);
  border: 1px solid rgba(63, 108, 149, 0.25);
  padding: 0 6px;
  border-radius: var(--radius-sm);
}

.endpoint-summary {
  font-size: var(--text-xl);
  margin-bottom: var(--space-3);
}

.endpoint-desc {
  font-family: var(--font-ui);
  font-size: var(--text-sm);
  color: var(--ink-muted);
  line-height: 1.7;
  margin-bottom: var(--space-6);
  white-space: pre-line;
}

.endpoint-block { margin-bottom: var(--space-6); }

.endpoint-block h3 {
  font-size: var(--text-base);
  font-weight: 600;
  margin-bottom: var(--space-3);
}

.content-type-label { margin-bottom: var(--space-3); }
.content-type-label code { font-size: var(--text-xs); color: var(--ink-muted); }

.param-table {
  width: 100%;
  border-collapse: collapse;
  font-family: var(--font-ui);
  font-size: var(--text-sm);
}

.param-table th {
  text-align: left;
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--ink-faint);
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--border-color);
}

.param-table td {
  padding: var(--space-3);
  border-bottom: 1px solid var(--paper-dark);
  color: var(--ink-light);
  vertical-align: top;
}

.param-table code { font-size: var(--text-xs); }

.param-required {
  font-family: var(--font-ui);
  font-size: 0.5625rem;
  font-weight: 600;
  text-transform: uppercase;
  color: var(--accent);
  margin-left: var(--space-2);
}

.param-type { color: var(--ink-faint); background: var(--paper-dark); }

.param-enum {
  margin-top: var(--space-2);
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}

.param-enum-label {
  font-family: var(--font-ui);
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--ink-faint);
}

.param-enum-value {
  color: var(--accent);
  background: var(--accent-light);
  padding: 2px 6px;
  border-radius: var(--radius-sm);
}

.response-row {
  display: flex;
  align-items: baseline;
  gap: var(--space-3);
  padding: var(--space-2) 0;
  font-family: var(--font-ui);
  font-size: var(--text-sm);
}

.response-status {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  font-weight: 600;
  padding: 2px 6px;
  border-radius: var(--radius-sm);
}

.response-status--success { background: var(--success-light); color: var(--success); }
.response-status--error { background: var(--danger-light); color: var(--danger); }

.response-desc { color: var(--ink-muted); }

.endpoint-code { min-width: 0; }

.code-sticky {
  position: sticky;
  top: 96px;
}

.response-tabs-block {
  margin-top: var(--space-2);
  border-radius: var(--radius-lg);
  overflow: hidden;
  border: 1px solid rgba(255, 255, 255, 0.04);
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.15);
}

.response-tabs-block :deep(.r-codeblock) {
  border: none !important;
  border-radius: 0 !important;
  box-shadow: none !important;
  margin-bottom: 0;
}

.response-tabs {
  display: flex;
  gap: var(--space-1);
  padding: var(--space-2) var(--space-5);
  background: var(--code-surface);
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}

.response-tab {
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  font-weight: 600;
  padding: 4px 10px;
  border-radius: var(--radius-sm);
  border: none;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
  background: transparent;
  color: #6b7a8d;
}

.response-tab:hover { color: #b4c1d1; }
.response-tab--active { color: #fff; }

.response-tab--active.response-tab--success {
  background: rgba(26, 138, 90, 0.25);
  color: #4ade80;
}

.response-tab--active.response-tab--error {
  background: rgba(212, 58, 58, 0.2);
  color: #f87171;
}

@media (max-width: 1024px) {
  .api-ref { grid-template-columns: 1fr; }
  .api-ref-nav {
    position: static;
    height: auto;
    border-right: none;
    border-bottom: 1px solid var(--border-color);
    padding: var(--space-4) var(--space-5);
  }
  .endpoint-section { grid-template-columns: 1fr; }
  .code-sticky { position: static; }
}
</style>
