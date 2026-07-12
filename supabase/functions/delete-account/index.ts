import { AwsClient } from "npm:aws4fetch";
import { createClient } from "jsr:@supabase/supabase-js@2";

const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type",
  "Access-Control-Allow-Methods": "POST, OPTIONS",
} as const;

function getEnv(name: string): string {
  const value = Deno.env.get(name);
  if (!value) {
    throw new Error(`Missing environment variable: ${name}`);
  }
  return value;
}

function jsonResponse(status: number, body: Record<string, unknown>) {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      ...CORS_HEADERS,
      "Content-Type": "application/json",
    },
  });
}

function parseBearerToken(authHeader: string | null): string | null {
  if (!authHeader) return null;
  const match = authHeader.match(/^Bearer\s+(.+)$/i);
  return match?.[1] ?? null;
}

function buildR2BaseUrl(accountId: string, bucket: string): string {
  return `https://${accountId}.r2.cloudflarestorage.com/${bucket}`;
}

function unescapeXml(text: string): string {
  return text
    .replaceAll("&amp;", "&")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&quot;", "\"")
    .replaceAll("&apos;", "'");
}

function extractTagText(xml: string, tagName: string): string | null {
  const match = xml.match(new RegExp(`<${tagName}>([^<]*)</${tagName}>`));
  return match?.[1] ? unescapeXml(match[1]) : null;
}

async function signedFetch(
  awsClient: AwsClient,
  input: string,
  init?: RequestInit,
): Promise<Response> {
  const signed = await awsClient.sign(input, init);
  return fetch(signed);
}

async function listObjectKeys(params: {
  awsClient: AwsClient;
  baseUrl: string;
  prefix: string;
}): Promise<string[]> {
  const keys: string[] = [];
  let continuationToken: string | null = null;

  while (true) {
    const url = new URL(params.baseUrl);
    url.searchParams.set("list-type", "2");
    url.searchParams.set("prefix", params.prefix);
    if (continuationToken) {
      url.searchParams.set("continuation-token", continuationToken);
    }

    const response = await signedFetch(params.awsClient, url.toString(), {
      method: "GET",
    });
    if (!response.ok) {
      throw new Error(`ListObjectsV2 failed with status ${response.status}`);
    }

    const xml = await response.text();

    // Supabase Edge Runtime 运行在 Deno 环境，没有浏览器里的 DOMParser；
    // 这里改用正则读取 S3 ListObjectsV2 的稳定 XML 结构，避免运行时崩溃。
    for (const match of xml.matchAll(/<Key>([^<]+)<\/Key>/g)) {
      const key = unescapeXml(match[1]);
      if (key) {
        keys.push(key);
      }
    }

    const isTruncated = extractTagText(xml, "IsTruncated") === "true";
    if (!isTruncated) {
      break;
    }

    continuationToken = extractTagText(xml, "NextContinuationToken");
    if (!continuationToken) {
      throw new Error("Missing NextContinuationToken in truncated response");
    }
  }

  return keys;
}

async function deleteObjectsUnderPrefix(params: {
  awsClient: AwsClient;
  baseUrl: string;
  prefix: string;
}) {
  const keys = await listObjectKeys(params);

  for (const key of keys) {
    const objectUrl = `${params.baseUrl}/${key}`;
    const response = await signedFetch(params.awsClient, objectUrl, {
      method: "DELETE",
    });

    // S3/R2 对不存在对象通常也返回成功；即使返回 404，也视为幂等删除已完成。
    if (!response.ok && response.status !== 404) {
      throw new Error(`Delete object failed for key ${key} with status ${response.status}`);
    }
  }
}

function isUserMissingError(error: unknown): boolean {
  if (!error || typeof error !== "object") return false;

  const message = "message" in error && typeof error.message === "string"
    ? error.message.toLowerCase()
    : "";
  const code = "code" in error && typeof error.code === "string"
    ? error.code.toLowerCase()
    : "";

  return message.includes("not found") || message.includes("user not found") || code === "user_not_found";
}

Deno.serve(async (request) => {
  if (request.method === "OPTIONS") {
    return new Response("ok", {
      status: 200,
      headers: CORS_HEADERS,
    });
  }

  if (request.method !== "POST") {
    return jsonResponse(400, { error: "Only POST is supported" });
  }

  try {
    const supabaseUrl = getEnv("SUPABASE_URL");
    const anonKey = getEnv("SUPABASE_ANON_KEY");
    const serviceRoleKey = getEnv("SUPABASE_SERVICE_ROLE_KEY");
    const accountId = getEnv("R2_ACCOUNT_ID");
    const accessKeyId = getEnv("R2_ACCESS_KEY_ID");
    const secretAccessKey = getEnv("R2_SECRET_ACCESS_KEY");
    const bucket = getEnv("R2_BUCKET");

    const token = parseBearerToken(request.headers.get("Authorization"));
    if (!token) {
      return jsonResponse(401, { error: "Missing bearer token" });
    }

    const authClient = createClient(supabaseUrl, anonKey, {
      auth: {
        persistSession: false,
        autoRefreshToken: false,
      },
    });
    const serviceClient = createClient(supabaseUrl, serviceRoleKey, {
      auth: {
        persistSession: false,
        autoRefreshToken: false,
      },
    });

    // 第 1 步：先用用户 JWT 解析出 user_id；失败直接 401。
    const {
      data: { user },
      error: userError,
    } = await authClient.auth.getUser(token);
    if (userError || !user) {
      return jsonResponse(401, { error: "Invalid token" });
    }

    const awsClient = new AwsClient({
      accessKeyId,
      secretAccessKey,
      service: "s3",
      region: "auto",
    });
    const baseUrl = buildR2BaseUrl(accountId, bucket);
    const userPrefix = `${user.id}/`;

    // 第 2 步：先清空 R2 前缀。这样如果对象删除失败，整个流程中止，调用方可安全重试。
    try {
      await deleteObjectsUnderPrefix({
        awsClient,
        baseUrl,
        prefix: userPrefix,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unknown error";
      return jsonResponse(500, { error: `step:r2_cleanup ${message}` });
    }

    // 第 3 步：显式删除业务表。虽然最终删除 auth 用户会触发级联，但这里先手动删表，
    // 目的是把“对象存储清理”和“数据库清理”顺序固定下来，R2 失败时也能直接重试。
    try {
      const tableNames = ["books", "reading_progress", "user_quota"] as const;

      for (const tableName of tableNames) {
        const { error } = await serviceClient
          .from(tableName)
          .delete()
          .eq("user_id", user.id);

        if (error) {
          throw new Error(`${tableName}: ${error.message}`);
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unknown error";
      return jsonResponse(500, { error: `step:database_cleanup ${message}` });
    }

    // 第 4 步：最后删除 auth 用户。若用户已不存在，视为幂等成功，避免重试时报错。
    try {
      const { error } = await serviceClient.auth.admin.deleteUser(user.id);
      if (error && !isUserMissingError(error)) {
        throw error;
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unknown error";
      return jsonResponse(500, { error: `step:auth_delete ${message}` });
    }

    return jsonResponse(200, { deleted: true });
  } catch (error) {
    const message = error instanceof Error ? error.message : "Unknown error";
    return jsonResponse(500, { error: `step:bootstrap ${message}` });
  }
});
