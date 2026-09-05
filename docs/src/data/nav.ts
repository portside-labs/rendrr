export interface DocLink {
  slug: string;
  title: string;
  description?: string;
}

export const DOCS: DocLink[] = [
  { slug: "", title: "Introduction", description: "What Rendrr is and what it does." },
  { slug: "getting-started", title: "Getting started", description: "Run Rendrr in one command." },
  { slug: "template-syntax", title: "Template syntax", description: "Handlebars dialect Rendrr understands." },
  { slug: "oauth", title: "OAuth 2.0", description: "Lock the API behind your identity provider." },
  { slug: "deployment", title: "Deployment", description: "Production deployment patterns." },
  { slug: "ai-template-skill", title: "AI template skill", description: "Generate templates by chatting with Claude." },
];

export const API_REFERENCE: DocLink = {
  slug: "api-reference",
  title: "API reference",
};
