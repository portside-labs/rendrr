import { ref, computed } from "vue";

export type CodeLanguageId = "curl" | "node" | "php" | "python" | "java" | "ruby" | "go";

export interface CodeLanguage {
  id: CodeLanguageId;
  label: string;
  highlightLang: string;
}

export const CODE_LANGUAGES: CodeLanguage[] = [
  { id: "curl", label: "cURL", highlightLang: "bash" },
  { id: "node", label: "Node.js", highlightLang: "javascript" },
  { id: "php", label: "PHP", highlightLang: "php" },
  { id: "python", label: "Python", highlightLang: "python" },
  { id: "java", label: "Java", highlightLang: "java" },
  { id: "ruby", label: "Ruby", highlightLang: "ruby" },
  { id: "go", label: "Go", highlightLang: "go" },
];

const selected = ref<CodeLanguageId>("curl");

export function useCodeLanguage() {
  function setLanguage(id: CodeLanguageId) {
    selected.value = id;
  }

  const current = computed(() => CODE_LANGUAGES.find((l) => l.id === selected.value)!);

  return {
    selected,
    current,
    languages: CODE_LANGUAGES,
    setLanguage,
  };
}
