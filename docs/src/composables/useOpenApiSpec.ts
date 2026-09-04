import { ref, onMounted, readonly } from "vue";
import { parse } from "yaml";
import type { CodeLanguageId } from "./useCodeLanguage";

export interface ApiEndpoint {
  id: string;
  method: string;
  path: string;
  summary: string;
  description: string;
  tag: string;
  requiredScope: string | null;
  parameters: ApiParam[];
  requestBody: ApiRequestBody | null;
  responses: ApiResponse[];
  curlExample: string;
  codeExamples: Record<CodeLanguageId, string>;
  responseExample: string;
}

export interface ApiParam {
  name: string;
  in: string;
  required: boolean;
  type: string;
  description: string;
  example?: string;
  enum?: string[];
}

export interface ApiRequestBody {
  contentType: string;
  required: boolean;
  properties: ApiParam[];
}

export interface ApiResponseExample {
  name: string;
  summary: string;
  value: any;
}

export interface ApiResponse {
  status: string;
  description: string;
  example?: any;
  examples: ApiResponseExample[];
}

interface RequestContext {
  method: string;
  url: string;
  contentType: string | null;
  exampleData: any | null;
  isMultipart: boolean;
  multipartFields: { key: string; value: string; isBinary: boolean }[];
}

function resolveRequestContext(method: string, path: string, endpoint: any, servers: any[]): RequestContext {
  const baseUrl = servers?.[0]?.url || "http://localhost:3000";

  let resolvedPath = path;
  const params: any[] = endpoint.parameters || [];
  for (const p of params) {
    if (p.in === "path" && p.example) {
      resolvedPath = resolvedPath.replace(`{${p.name}}`, p.example);
    }
  }

  const url = `${baseUrl}${resolvedPath}`;
  const content = endpoint.requestBody?.content;
  const contentType = content ? Object.keys(content)[0] : null;
  const isMultipart = contentType === "multipart/form-data";

  let exampleData: any = null;
  let multipartFields: { key: string; value: string; isBinary: boolean }[] = [];

  if (content && contentType) {
    if (isMultipart) {
      const schema = content[contentType]?.schema;
      const props = schema?.properties || {};
      multipartFields = Object.entries(props).map(([key, val]: [string, any]) => ({
        key,
        value: val.format === "binary" ? "template.docx" : (val.example || key),
        isBinary: val.format === "binary",
      }));
    } else {
      const namedExamples = content[contentType]?.examples;
      exampleData = content[contentType]?.example;
      if (namedExamples) {
        const firstKey = Object.keys(namedExamples)[0];
        if (firstKey) exampleData = namedExamples[firstKey].value;
      }
    }
  }

  return { method: method.toUpperCase(), url, contentType, exampleData, isMultipart, multipartFields };
}

function buildCurl(ctx: RequestContext): string {
  const { method, url, exampleData, isMultipart, multipartFields } = ctx;

  if (isMultipart) {
    const fields = multipartFields.map((f) =>
      f.isBinary ? `-F "file=@${f.value}"` : `-F "${f.key}=${f.value}"`
    );
    return `curl -X ${method} ${url} \\\n  ${fields.join(" \\\n  ")}`;
  }

  if (exampleData) {
    const json = JSON.stringify(exampleData, null, 4);
    return `curl -X ${method} ${url} \\\n  -H "Content-Type: application/json" \\\n  -d '${json}'`;
  }

  if (method === "GET") {
    return `curl ${url}`;
  }
  return `curl -X ${method} ${url}`;
}

function buildNode(ctx: RequestContext): string {
  const { method, url, exampleData, isMultipart, multipartFields } = ctx;

  if (isMultipart) {
    const fields = multipartFields.map((f) => {
      if (f.isBinary) return `formData.append("file", fs.createReadStream("${f.value}"));`;
      return `formData.append("${f.key}", "${f.value}");`;
    });
    return `import fs from "fs";
import FormData from "form-data";

const formData = new FormData();
${fields.join("\n")}

const response = await fetch("${url}", {
    method: "${method}",
    headers: formData.getHeaders(),
    body: formData,
});

const data = await response.json();
console.log(data);`;
  }

  if (exampleData) {
    const json = JSON.stringify(exampleData, null, 4);
    return `const response = await fetch("${url}", {
    method: "${method}",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(${json}),
});

const data = await response.json();
console.log(data);`;
  }

  if (method === "DELETE") {
    return `const response = await fetch("${url}", { method: "DELETE" });
console.log(response.status); // 204`;
  }

  return `const response = await fetch("${url}");
const data = await response.json();
console.log(data);`;
}

function buildPhp(ctx: RequestContext): string {
  const { method, url, exampleData, isMultipart, multipartFields } = ctx;

  if (isMultipart) {
    const fields = multipartFields.map((f) => {
      if (f.isBinary) {
        return `            [
                'name'     => 'file',
                'contents' => fopen('${f.value}', 'r'),
                'filename' => '${f.value}',
            ],`;
      }
      return `            [
                'name'     => '${f.key}',
                'contents' => '${f.value}',
            ],`;
    });
    return `<?php

use GuzzleHttp\\Client;

$client = new Client();

$response = $client->request('${method}', '${url}', [
    'multipart' => [
${fields.join("\n")}
    ],
]);

echo $response->getBody();`;
  }

  if (exampleData) {
    const phpArray = jsonToPhpArray(exampleData, 1);
    return `<?php

use GuzzleHttp\\Client;

$client = new Client();

$response = $client->request('${method}', '${url}', [
    'headers' => [ 'Content-Type' => 'application/json' ],
    'json' => ${phpArray},
]);

echo $response->getBody();`;
  }

  if (method === "DELETE") {
    return `<?php

use GuzzleHttp\\Client;

$client = new Client();

$response = $client->request('DELETE', '${url}');

echo $response->getStatusCode(); // 204`;
  }

  return `<?php

use GuzzleHttp\\Client;

$client = new Client();

$response = $client->request('GET', '${url}');

echo $response->getBody();`;
}

function jsonToPhpArray(obj: any, depth: number): string {
  const indent = "    ".repeat(depth);
  const innerIndent = "    ".repeat(depth + 1);

  if (Array.isArray(obj)) {
    if (obj.length === 0) return "[]";
    const items = obj.map((item) => `${innerIndent}${jsonToPhpArray(item, depth + 1)},`);
    return `[\n${items.join("\n")}\n${indent}]`;
  }

  if (obj !== null && typeof obj === "object") {
    const entries = Object.entries(obj);
    if (entries.length === 0) return "[]";
    const items = entries.map(([key, val]) => `${innerIndent}'${key}' => ${jsonToPhpArray(val, depth + 1)},`);
    return `[\n${items.join("\n")}\n${indent}]`;
  }

  if (typeof obj === "string") return `'${obj}'`;
  if (typeof obj === "boolean") return obj ? "true" : "false";
  if (obj === null) return "null";
  return String(obj);
}

function buildPython(ctx: RequestContext): string {
  const { method, url, exampleData, isMultipart, multipartFields } = ctx;
  const lower = method.toLowerCase();

  if (isMultipart) {
    const fields = multipartFields.map((f) => {
      if (f.isBinary) return `    "file": open("${f.value}", "rb"),`;
      return `    "${f.key}": (None, "${f.value}"),`;
    });
    return `import requests

response = requests.${lower}(
    "${url}",
    files={
${fields.join("\n")}
    },
)

print(response.json())`;
  }

  if (exampleData) {
    const json = JSON.stringify(exampleData, null, 4);
    return `import requests

response = requests.${lower}(
    "${url}",
    json=${json},
)

print(response.json())`;
  }

  if (method === "DELETE") {
    return `import requests

response = requests.delete("${url}")
print(response.status_code)  # 204`;
  }

  return `import requests

response = requests.get("${url}")
print(response.json())`;
}

function buildJava(ctx: RequestContext): string {
  const { method, url, exampleData, isMultipart, multipartFields } = ctx;

  if (isMultipart) {
    const fields = multipartFields.map((f) => {
      if (f.isBinary)
        return `        .addFormDataPart("file", "${f.value}",\n            RequestBody.create(MediaType.parse("application/octet-stream"),\n                new File("${f.value}")))`;
      return `        .addFormDataPart("${f.key}", "${f.value}")`;
    });
    return `import okhttp3.*;
import java.io.File;

OkHttpClient client = new OkHttpClient();

RequestBody body = new MultipartBody.Builder()
        .setType(MultipartBody.FORM)
${fields.join("\n")}
        .build();

Request request = new Request.Builder()
        .url("${url}")
        .${method.toLowerCase()}(body)
        .build();

try (Response response = client.newCall(request).execute()) {
    System.out.println(response.body().string());
}`;
  }

  if (exampleData) {
    const json = JSON.stringify(exampleData, null, 4)
      .split("\n")
      .map((line, i) => (i === 0 ? line : `        ${line}`))
      .join("\n");
    return `import okhttp3.*;

OkHttpClient client = new OkHttpClient();

MediaType JSON = MediaType.get("application/json");

String json = """
        ${json}
        """;

RequestBody body = RequestBody.create(json, JSON);

Request request = new Request.Builder()
        .url("${url}")
        .${method.toLowerCase()}(body)
        .build();

try (Response response = client.newCall(request).execute()) {
    System.out.println(response.body().string());
}`;
  }

  if (method === "DELETE") {
    return `import okhttp3.*;

OkHttpClient client = new OkHttpClient();

Request request = new Request.Builder()
        .url("${url}")
        .delete()
        .build();

try (Response response = client.newCall(request).execute()) {
    System.out.println(response.code()); // 204
}`;
  }

  return `import okhttp3.*;

OkHttpClient client = new OkHttpClient();

Request request = new Request.Builder()
        .url("${url}")
        .build();

try (Response response = client.newCall(request).execute()) {
    System.out.println(response.body().string());
}`;
}

function buildRuby(ctx: RequestContext): string {
  const { method, url, exampleData, isMultipart, multipartFields } = ctx;

  if (isMultipart) {
    const fields = multipartFields.map((f) => {
      if (f.isBinary) return `  ["file", File.open("${f.value}", "rb"), { filename: "${f.value}" }],`;
      return `  ["${f.key}", "${f.value}"],`;
    });
    return `require "net/http"
require "uri"

uri = URI("${url}")
http = Net::HTTP.new(uri.host, uri.port)
http.use_ssl = (uri.scheme == "https")

form_data = [
${fields.join("\n")}
]

request = Net::HTTP::Post.new(uri)
request.set_form(form_data, "multipart/form-data")

response = http.request(request)
puts response.body`;
  }

  if (exampleData) {
    const json = JSON.stringify(exampleData, null, 2)
      .split("\n")
      .map((line, i) => (i === 0 ? line : `  ${line}`))
      .join("\n");
    return `require "net/http"
require "uri"
require "json"

uri = URI("${url}")
http = Net::HTTP.new(uri.host, uri.port)
http.use_ssl = (uri.scheme == "https")

request = Net::HTTP::${capitalize(method)}.new(uri)
request["Content-Type"] = "application/json"
request.body = ${json}.to_json

response = http.request(request)
puts response.body`;
  }

  if (method === "DELETE") {
    return `require "net/http"
require "uri"

uri = URI("${url}")
http = Net::HTTP.new(uri.host, uri.port)
http.use_ssl = (uri.scheme == "https")

request = Net::HTTP::Delete.new(uri)
response = http.request(request)
puts response.code # 204`;
  }

  return `require "net/http"
require "uri"

uri = URI("${url}")
http = Net::HTTP.new(uri.host, uri.port)
http.use_ssl = (uri.scheme == "https")

request = Net::HTTP::Get.new(uri)
response = http.request(request)
puts response.body`;
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1).toLowerCase();
}

function buildGo(ctx: RequestContext): string {
  const { method, url, exampleData, isMultipart } = ctx;

  if (isMultipart) {
    return `package main

import (
    "bytes"
    "fmt"
    "io"
    "mime/multipart"
    "net/http"
    "os"
)

func main() {
    body := &bytes.Buffer{}
    writer := multipart.NewWriter(body)

    file, _ := os.Open("template.docx")
    defer file.Close()

    part, _ := writer.CreateFormFile("file", "template.docx")
    io.Copy(part, file)
    writer.Close()

    req, _ := http.NewRequest("${method}", "${url}", body)
    req.Header.Set("Content-Type", writer.FormDataContentType())

    resp, _ := http.DefaultClient.Do(req)
    defer resp.Body.Close()

    respBody, _ := io.ReadAll(resp.Body)
    fmt.Println(string(respBody))
}`;
  }

  if (exampleData) {
    const json = JSON.stringify(exampleData, null, 4);
    return `package main

import (
    "fmt"
    "io"
    "net/http"
    "strings"
)

func main() {
    payload := strings.NewReader(\`${json}\`)

    req, _ := http.NewRequest("${method}", "${url}", payload)
    req.Header.Set("Content-Type", "application/json")

    resp, _ := http.DefaultClient.Do(req)
    defer resp.Body.Close()

    body, _ := io.ReadAll(resp.Body)
    fmt.Println(string(body))
}`;
  }

  if (method === "DELETE") {
    return `package main

import (
    "fmt"
    "net/http"
)

func main() {
    req, _ := http.NewRequest("DELETE", "${url}", nil)
    resp, _ := http.DefaultClient.Do(req)
    defer resp.Body.Close()

    fmt.Println(resp.StatusCode) // 204
}`;
  }

  return `package main

import (
    "fmt"
    "io"
    "net/http"
)

func main() {
    resp, _ := http.Get("${url}")
    defer resp.Body.Close()

    body, _ := io.ReadAll(resp.Body)
    fmt.Println(string(body))
}`;
}

function buildCodeExamples(method: string, path: string, endpoint: any, servers: any[]): Record<CodeLanguageId, string> {
  const ctx = resolveRequestContext(method, path, endpoint, servers);
  return {
    curl: buildCurl(ctx),
    node: buildNode(ctx),
    php: buildPhp(ctx),
    python: buildPython(ctx),
    java: buildJava(ctx),
    ruby: buildRuby(ctx),
    go: buildGo(ctx),
  };
}

function resolveRef(ref: string, spec: any): any {
  const parts = ref.replace("#/", "").split("/");
  let node = spec;
  for (const part of parts) {
    node = node?.[part];
    if (!node) return {};
  }
  return node;
}

function extractEndpoints(spec: any): ApiEndpoint[] {
  const endpoints: ApiEndpoint[] = [];
  const paths = spec.paths || {};
  const servers = spec.servers || [];

  for (const [path, methods] of Object.entries(paths) as any) {
    for (const [method, endpoint] of Object.entries(methods as any) as any) {
      if (["get", "post", "put", "patch", "delete"].indexOf(method) === -1) continue;

      const tag = endpoint.tags?.[0] || "General";
      const id = endpoint.operationId || `${method}-${path}`.replace(/[^a-z0-9]/gi, "-");

      const parameters: ApiParam[] = (endpoint.parameters || []).map((p: any) => ({
        name: p.name,
        in: p.in,
        required: p.required || false,
        type: p.schema?.type || "string",
        description: p.description || "",
        example: p.example,
        enum: p.schema?.enum,
      }));

      let requestBody: ApiRequestBody | null = null;
      if (endpoint.requestBody) {
        const content = endpoint.requestBody.content || {};
        const contentType = Object.keys(content)[0] || "application/json";
        const rawSchema = content[contentType]?.schema;
        const schema = rawSchema?.$ref ? resolveRef(rawSchema.$ref, spec) : rawSchema;
        const props = schema?.properties || {};
        requestBody = {
          contentType,
          required: endpoint.requestBody.required || false,
          properties: Object.entries(props).map(([name, val]: [string, any]) => ({
            name,
            in: "body",
            required: (schema?.required || []).includes(name),
            type: val.format === "binary" ? "file" : val.type || "string",
            description: val.description || "",
            example: val.example,
            enum: val.enum,
          })),
        };
      }

      const responses: ApiResponse[] = Object.entries(endpoint.responses || {}).map(
        ([status, rawResp]: [string, any]) => {
          const resp = rawResp.$ref ? resolveRef(rawResp.$ref, spec) : rawResp;
          const content = Object.values(resp.content || {})[0] as any;
          const singleExample = content?.example;
          const namedExamples = content?.examples;

          const examples: ApiResponseExample[] = [];
          if (namedExamples) {
            for (const [name, ex] of Object.entries(namedExamples) as any) {
              examples.push({ name, summary: ex.summary || name, value: ex.value });
            }
          } else if (singleExample) {
            examples.push({ name: status, summary: resp.description || "", value: singleExample });
          }

          return {
            status,
            description: resp.description || "",
            example: singleExample,
            examples,
          };
        }
      );

      const successResp = responses.find((r) => r.status.startsWith("2"));
      const responseExample = successResp?.example ? JSON.stringify(successResp.example, null, 2) : "";

      const codeExamples = buildCodeExamples(method, path, endpoint, servers);

      endpoints.push({
        id,
        method: method.toUpperCase(),
        path,
        summary: endpoint.summary || "",
        description: endpoint.description || "",
        tag,
        requiredScope: endpoint["x-required-scope"] || null,
        parameters,
        requestBody,
        responses,
        curlExample: codeExamples.curl,
        codeExamples,
        responseExample,
      });
    }
  }

  return endpoints;
}

function specUrl(): string {
  const base = import.meta.env.BASE_URL.replace(/\/$/, "");
  return `${base}/openapi.yaml`;
}

export function useOpenApiSpec() {
  const spec = ref<any>(null);
  const endpoints = ref<ApiEndpoint[]>([]);
  const tags = ref<string[]>([]);
  const loading = ref(true);
  const error = ref("");

  async function load() {
    loading.value = true;
    error.value = "";
    try {
      const res = await fetch(specUrl());
      if (!res.ok) throw new Error(`Failed to fetch spec: ${res.status}`);
      const text = await res.text();
      spec.value = parse(text);
      endpoints.value = extractEndpoints(spec.value);
      tags.value = [...new Set(endpoints.value.map((e) => e.tag))];
    } catch (e: any) {
      error.value = e.message || "Failed to load API spec.";
    } finally {
      loading.value = false;
    }
  }

  onMounted(load);

  return {
    spec: readonly(spec),
    endpoints: readonly(endpoints),
    tags: readonly(tags),
    loading: readonly(loading),
    error: readonly(error),
  };
}
