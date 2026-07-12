/**
 * sign-url Edge Function 协议摘要：
 * - POST JSON: { op: "put" | "get", key: "books/<sha256>.pdf" | "covers/<sha256>.jpg", bytes?: number }
 * - 200: { url: string, expires_at: number }
 * - 401: JWT 缺失或无效
 * - 403: key 形态非法
 * - 413: 配额超限或单文件超限，配额错误返回 { error, bytes_used, bytes_limit }
 * - 400: 请求参数错误
 */

import { AwsClient } from "npm:aws4fetch";
import { createClient } from "jsr:@supabase/supabase-js@2";

const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type",
  "Access-Control-Allow-Methods": "POST, OPTIONS",
} as const;

const SIGN_EXPIRES_SECONDS = 900;
const SINGLE_FILE_LIMIT_BYTES = 524_288_000;
const BOOK_KEY_RE = /^books\/[0-9a-f]{64}\.pdf$/;
const COVER_KEY_RE = /^covers\/[0-9a-f]{64}\.jpg$/;

type RequestBody = {
  op?: "put" | "get";
  key?: string;
  bytes?: number;
};

type QuotaRow = {
  bytes_used: number;
  bytes_limit: number;
};

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

function isAllowedKey(key: string): boolean {
  return BOOK_KEY_RE.test(key) || COVER_KEY_RE.test(key);
}

function parseBearerToken(authHeader: string | null): string | null {
  if (!authHeader) return null;
  const match = authHeader.match(/^Bearer\s+(.+)$/i);
  return match?.[1] ?? null;
}

function isPositiveInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value > 0;
}

async function buildSignedUrl(params: {
  accountId: string;
  bucket: string;
  fullKey: string;
  method: "PUT" | "GET";
  awsClient: AwsClient;
}): Promise<string> {
  const url = new URL(
    `https://${params.accountId}.r2.cloudflarestorage.com/${params.bucket}/${params.fullKey}`,
  );
  url.searchParams.set("X-Amz-Expires", String(SIGN_EXPIRES_SECONDS));

  const signed = await params.awsClient.sign(url.toString(), {
    method: params.method,
    aws: {
      signQuery: true,
    },
  });

  if (signed instanceof Request) {
    return signed.url;
  }

  return signed.toString();
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

    const {
      data: { user },
      error: userError,
    } = await authClient.auth.getUser(token);
    if (userError || !user) {
      return jsonResponse(401, { error: "Invalid token" });
    }

    let body: RequestBody;
    try {
      body = await request.json();
    } catch {
      return jsonResponse(400, { error: "Invalid JSON body" });
    }

    const { op, key, bytes } = body;
    if ((op !== "put" && op !== "get") || typeof key !== "string") {
      return jsonResponse(400, { error: "Invalid op or key" });
    }

    if (!isAllowedKey(key)) {
      return jsonResponse(403, { error: "Key is not allowed" });
    }

    if (op === "put" && !isPositiveInteger(bytes)) {
      return jsonResponse(400, { error: "bytes must be a positive integer for put" });
    }

    const fullKey = `${user.id}/${key}`;

    if (op === "put") {
      const uploadBytes = bytes as number;

      if (uploadBytes > SINGLE_FILE_LIMIT_BYTES) {
        return jsonResponse(413, {
          error: "Single file size exceeds limit",
          bytes_used: null,
          bytes_limit: SINGLE_FILE_LIMIT_BYTES,
        });
      }

      const { data: quotaData, error: quotaError } = await serviceClient
        .from("user_quota")
        .select("bytes_used, bytes_limit")
        .eq("user_id", user.id)
        .maybeSingle();
      const quota = quotaData as QuotaRow | null;

      if (quotaError || !quota) {
        return jsonResponse(400, { error: "Quota record not found" });
      }

      if (quota.bytes_used + uploadBytes > quota.bytes_limit) {
        return jsonResponse(413, {
          error: "Quota exceeded",
          bytes_used: quota.bytes_used,
          bytes_limit: quota.bytes_limit,
        });
      }

      // 简化模型：签发上传 URL 时立即计入配额，后续由对账任务修正未实际落库的差异。
      const { error: updateError } = await serviceClient
        .from("user_quota")
        .update({ bytes_used: quota.bytes_used + uploadBytes })
        .eq("user_id", user.id);

      if (updateError) {
        return jsonResponse(400, { error: "Failed to update quota" });
      }
    }

    const awsClient = new AwsClient({
      accessKeyId,
      secretAccessKey,
      service: "s3",
      region: "auto",
    });

    const expiresAt = Date.now() + SIGN_EXPIRES_SECONDS * 1000;
    const url = await buildSignedUrl({
      accountId,
      bucket,
      fullKey,
      method: op === "put" ? "PUT" : "GET",
      awsClient,
    });

    return jsonResponse(200, {
      url,
      expires_at: expiresAt,
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : "Unknown error";
    return jsonResponse(400, { error: message });
  }
});
