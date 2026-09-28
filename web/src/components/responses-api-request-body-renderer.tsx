import { type ReactNode } from "react"
import { FileIcon, ImageIcon, PaperclipIcon, WrenchIcon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  base64PayloadSize,
  contentParts,
  contentText,
  displayValue,
  imageUrl,
  inputItems,
  parseJsonValue,
  recordValue,
  type JsonRecord,
} from "@/lib/responses-api-request"

type Locale = "zh" | "en"

const copy = {
  zh: {
    title: "Responses 请求正文",
    description:
      "按 v1/responses 语义整理已保留的请求预览；原始 JSON 始终可供核对。",
    structured: "结构化视图",
    rawJson: "原始 JSON",
    previewTruncated: "预览已截断",
    requestOptions: "请求选项",
    instructions: "Instructions",
    input: "输入",
    inputCount: (count: number) => `${count} 个输入项`,
    tools: "工具",
    toolCount: (count: number) => `${count} 个工具`,
    additionalTools: "附加工具",
    attachments: "附件与媒体",
    otherOptions: "其他请求参数",
    noInput: "请求未提供 input；请在原始 JSON 中检查完整正文。",
    noOtherOptions: "没有其他请求参数。",
    message: "消息",
    content: "内容",
    image: "图片",
    file: "文件",
    audio: "音频",
    tool: "工具",
    call: "调用",
    unknown: "未识别内容",
    detail: "详情",
    name: "名称",
    type: "类型",
    role: "角色",
    required: "必填",
    optional: "可选",
    parameters: "参数",
    descriptionLabel: "说明",
    identifier: "标识",
    url: "URL",
    filename: "文件名",
    mediaType: "媒体类型",
    imageDetail: "图片细节",
    payload: "内联数据",
    payloadSize: "载荷大小",
    unavailable: "未提供",
    openFile: "打开文件",
    openImage: "打开图片",
    functionTool: "函数",
  },
  en: {
    title: "Responses request body",
    description:
      "A structured v1/responses view of the retained request preview. Raw JSON remains available for verification.",
    structured: "Structured view",
    rawJson: "Raw JSON",
    previewTruncated: "Preview truncated",
    requestOptions: "Request options",
    instructions: "Instructions",
    input: "Input",
    inputCount: (count: number) =>
      `${count} input ${count === 1 ? "item" : "items"}`,
    tools: "Tools",
    toolCount: (count: number) => `${count} ${count === 1 ? "tool" : "tools"}`,
    additionalTools: "Additional tools",
    attachments: "Attachments and media",
    otherOptions: "Other request options",
    noInput:
      "The request does not include input. Inspect the raw JSON for the complete body.",
    noOtherOptions: "No other request options.",
    message: "Message",
    content: "Content",
    image: "Image",
    file: "File",
    audio: "Audio",
    tool: "Tool",
    call: "Call",
    unknown: "Unrecognized content",
    detail: "Details",
    name: "Name",
    type: "Type",
    role: "Role",
    required: "Required",
    optional: "Optional",
    parameters: "Parameters",
    descriptionLabel: "Description",
    identifier: "Identifier",
    url: "URL",
    filename: "Filename",
    mediaType: "Media type",
    imageDetail: "Image detail",
    payload: "Inline data",
    payloadSize: "Payload size",
    unavailable: "Not provided",
    openFile: "Open file",
    openImage: "Open image",
    functionTool: "Function",
  },
} as const

const requestOptionKeys = [
  "model",
  "previous_response_id",
  "conversation",
  "store",
  "stream",
  "background",
  "max_output_tokens",
  "max_tool_calls",
  "parallel_tool_calls",
  "tool_choice",
  "reasoning",
  "text",
  "modalities",
  "truncation",
  "temperature",
  "top_p",
  "prompt",
  "prompt_cache_key",
  "prompt_cache_options",
  "service_tier",
  "safety_identifier",
  "metadata",
  "include",
  "context_management",
  "moderation",
  "top_logprobs",
  "user",
] as const

const renderedKeys = new Set([
  "input",
  "instructions",
  "tools",
  ...requestOptionKeys,
])

export function ResponsesAPIRequestBodyRenderer({
  request,
  locale,
  truncated = false,
}: {
  request: unknown
  locale: Locale
  truncated?: boolean
}) {
  const t = copy[locale]
  const parsed = parseJsonValue(request)
  const record = recordValue(parsed)
  const raw = displayValue(parsed)

  if (!record) {
    return (
      <Card className="[--card-spacing:--spacing(5)]">
        <CardHeader>
          <CardTitle className="text-base leading-6 font-semibold">
            {t.title}
          </CardTitle>
          <CardDescription>{t.description}</CardDescription>
        </CardHeader>
        <CardContent>
          {truncated && <Badge variant="secondary">{t.previewTruncated}</Badge>}
          <RawJson className={truncated ? "mt-3" : undefined} value={raw} />
        </CardContent>
      </Card>
    )
  }

  const input = record.input
  const inputs = input === undefined ? [] : inputItems(input)
  const tools = Array.isArray(record.tools) ? record.tools : []
  const requestOptions = requestOptionKeys.flatMap((key) =>
    record[key] === undefined ? [] : [[key, record[key]] as const]
  )
  const otherOptions = Object.entries(record).filter(
    ([key]) => !renderedKeys.has(key)
  )

  return (
    <Card className="[--card-spacing:--spacing(5)]">
      <CardHeader>
        <CardTitle className="text-base leading-6 font-semibold">
          {t.title}
        </CardTitle>
        <CardDescription>{t.description}</CardDescription>
      </CardHeader>
      <CardContent>
        <Tabs defaultValue="structured">
          <TabsList aria-label={t.title}>
            <TabsTrigger value="structured">{t.structured}</TabsTrigger>
            <TabsTrigger value="raw">{t.rawJson}</TabsTrigger>
          </TabsList>
          <TabsContent value="structured" className="pt-5">
            <div className="flex flex-col gap-6">
              {truncated && (
                <Badge variant="secondary">{t.previewTruncated}</Badge>
              )}
              {requestOptions.length > 0 && (
                <RequestSection title={t.requestOptions}>
                  <OptionList rows={requestOptions} />
                </RequestSection>
              )}
              {record.instructions !== undefined && (
                <>
                  <Separator />
                  <RequestSection title={t.instructions}>
                    <NaturalLanguageValue value={record.instructions} />
                  </RequestSection>
                </>
              )}
              <Separator />
              <RequestSection
                title={t.input}
                description={
                  inputs.length > 0 ? t.inputCount(inputs.length) : undefined
                }
              >
                {input === undefined ? (
                  <p className="text-sm leading-6 text-muted-foreground">
                    {t.noInput}
                  </p>
                ) : (
                  <InputItems items={inputs} locale={locale} />
                )}
              </RequestSection>
              {record.tools !== undefined && (
                <>
                  <Separator />
                  <RequestSection
                    title={t.tools}
                    description={t.toolCount(tools.length)}
                  >
                    <ToolList tools={tools} locale={locale} />
                  </RequestSection>
                </>
              )}
              <Separator />
              <RequestSection title={t.otherOptions}>
                {otherOptions.length === 0 ? (
                  <p className="text-sm leading-6 text-muted-foreground">
                    {t.noOtherOptions}
                  </p>
                ) : (
                  <OptionList rows={otherOptions} />
                )}
              </RequestSection>
            </div>
          </TabsContent>
          <TabsContent value="raw" className="pt-5">
            <RawJson value={raw} />
          </TabsContent>
        </Tabs>
      </CardContent>
    </Card>
  )
}

function RequestSection({
  title,
  description,
  children,
}: {
  title: string
  description?: string
  children: ReactNode
}) {
  return (
    <section className="flex min-w-0 flex-col gap-3">
      <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <h3 className="text-sm leading-5 font-medium">{title}</h3>
        {description && (
          <span className="text-xs text-muted-foreground">{description}</span>
        )}
      </div>
      {children}
    </section>
  )
}

function OptionList({
  rows,
}: {
  rows: readonly (readonly [string, unknown])[]
}) {
  return (
    <dl className="grid gap-x-6 gap-y-3 sm:grid-cols-2 lg:grid-cols-3">
      {rows.map(([label, value]) => (
        <div key={label} className="min-w-0 border-b pb-3">
          <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
          <dd className="mt-1 font-mono text-xs break-all whitespace-pre-wrap">
            {displayValue(value)}
          </dd>
        </div>
      ))}
    </dl>
  )
}

function InputItems({ items, locale }: { items: unknown[]; locale: Locale }) {
  const t = copy[locale]
  return (
    <ol className="overflow-hidden rounded-md border">
      {items.map((item, index) => {
        const record = recordValue(item)
        const role =
          stringValue(record?.role) || stringValue(record?.type) || t.content
        const content = record?.content ?? item
        return (
          <li key={index} className="min-w-0 border-b last:border-b-0">
            <div className="flex flex-wrap items-center gap-2 bg-muted/40 px-3 py-2">
              <Badge variant="secondary">{role}</Badge>
              <span className="text-xs text-muted-foreground">
                #{index + 1}
              </span>
            </div>
            <div className="min-w-0 p-3">
              <ContentValue value={content} locale={locale} />
            </div>
          </li>
        )
      })}
    </ol>
  )
}

function ContentValue({ value, locale }: { value: unknown; locale: Locale }) {
  if (typeof value === "string") return <NaturalLanguageValue value={value} />
  if (Array.isArray(value)) {
    return (
      <div className="flex min-w-0 flex-col gap-3">
        {contentParts(value).map((part, index) => (
          <ContentPart key={index} value={part} locale={locale} />
        ))}
      </div>
    )
  }
  return <ContentPart value={value} locale={locale} />
}

function ContentPart({ value, locale }: { value: unknown; locale: Locale }) {
  const t = copy[locale]
  const record = recordValue(value)
  const type = stringValue(record?.type)
  const text = contentText(value)
  if (!record) return <RawJson value={displayValue(value)} />
  if (type === "input_text" || type === "text") {
    return <NaturalLanguageValue value={text ?? displayValue(value)} />
  }
  if (type === "input_image" || type === "image_url") {
    return <ImageAttachment record={record} locale={locale} />
  }
  if (type === "input_file")
    return <FileAttachment record={record} locale={locale} />
  if (type === "input_audio")
    return <AudioAttachment record={record} locale={locale} />
  if (type === "additional_tools")
    return <AdditionalToolsInput record={record} locale={locale} />
  if (type?.includes("call") || type === "function_call_output") {
    return <CallAttachment record={record} locale={locale} />
  }
  if (text !== undefined) return <NaturalLanguageValue value={text} />
  return <StructuredContent record={record} label={type || t.unknown} />
}

function AdditionalToolsInput({
  record,
  locale,
}: {
  record: JsonRecord
  locale: Locale
}) {
  const t = copy[locale]
  const tools = Array.isArray(record.tools) ? record.tools : []
  return (
    <div className="flex min-w-0 flex-col gap-3">
      <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <div className="flex items-center gap-2">
          <WrenchIcon className="size-4 text-muted-foreground" />
          <span className="text-sm font-medium">{t.additionalTools}</span>
        </div>
        <span className="text-xs text-muted-foreground">
          {t.toolCount(tools.length)}
        </span>
      </div>
      <ToolList tools={tools} locale={locale} />
    </div>
  )
}

function NaturalLanguageValue({ value }: { value: unknown }) {
  return (
    <p className="max-w-[75ch] text-sm leading-6 break-words whitespace-pre-wrap">
      {typeof value === "string" ? value : displayValue(value)}
    </p>
  )
}

function ImageAttachment({
  record,
  locale,
}: {
  record: JsonRecord
  locale: Locale
}) {
  const t = copy[locale]
  const source = imageUrl(record.image_url ?? record.url)
  const preview = source?.startsWith("data:image/") ? source : undefined
  const detail = stringValue(record.detail)
  return (
    <div className="flex min-w-0 flex-col gap-3 rounded-md border bg-muted/20 p-3">
      <AttachmentHeading
        icon={<ImageIcon />}
        title={t.image}
        type={stringValue(record.type)}
      />
      {preview ? (
        <img
          className="max-h-64 max-w-full self-start rounded-md border bg-background object-contain"
          src={preview}
          alt={t.image}
        />
      ) : source ? (
        <a
          className="w-fit text-sm font-medium underline-offset-4 hover:underline focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
          href={source}
          target="_blank"
          rel="noreferrer"
        >
          {t.openImage}
        </a>
      ) : (
        <p className="text-sm text-muted-foreground">{t.unavailable}</p>
      )}
      <AttachmentFields
        rows={[
          [t.url, stringValue(record.image_url) || stringValue(record.url)],
          [t.imageDetail, detail],
        ]}
      />
    </div>
  )
}

function FileAttachment({
  record,
  locale,
}: {
  record: JsonRecord
  locale: Locale
}) {
  const t = copy[locale]
  const fileUrl = stringValue(record.file_url)
  const payloadSize = base64PayloadSize(record.file_data)
  return (
    <div className="flex min-w-0 flex-col gap-3 rounded-md border bg-muted/20 p-3">
      <AttachmentHeading
        icon={<PaperclipIcon />}
        title={t.file}
        type={stringValue(record.type)}
      />
      <AttachmentFields
        rows={[
          [t.filename, stringValue(record.filename)],
          [t.identifier, stringValue(record.file_id)],
          [t.url, fileUrl],
          [t.payload, record.file_data === undefined ? undefined : t.payload],
          [
            t.payloadSize,
            payloadSize === undefined
              ? undefined
              : formatBytes(payloadSize, locale),
          ],
        ]}
      />
      {safeHttpUrl(fileUrl) && (
        <a
          className="w-fit text-sm font-medium underline-offset-4 hover:underline focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
          href={fileUrl}
          target="_blank"
          rel="noreferrer"
        >
          {t.openFile}
        </a>
      )}
    </div>
  )
}

function AudioAttachment({
  record,
  locale,
}: {
  record: JsonRecord
  locale: Locale
}) {
  const t = copy[locale]
  const audio = recordValue(record.input_audio) ?? record
  const payloadSize = base64PayloadSize(audio.data)
  return (
    <div className="flex min-w-0 flex-col gap-3 rounded-md border bg-muted/20 p-3">
      <AttachmentHeading
        icon={<FileIcon />}
        title={t.audio}
        type={stringValue(record.type)}
      />
      <AttachmentFields
        rows={[
          [t.mediaType, stringValue(audio.format)],
          [t.payload, audio.data === undefined ? undefined : t.payload],
          [
            t.payloadSize,
            payloadSize === undefined
              ? undefined
              : formatBytes(payloadSize, locale),
          ],
        ]}
      />
    </div>
  )
}

function CallAttachment({
  record,
  locale,
}: {
  record: JsonRecord
  locale: Locale
}) {
  const t = copy[locale]
  const name = stringValue(record.name)
  const payload = record.arguments ?? record.output
  return (
    <div className="flex min-w-0 flex-col gap-3 rounded-md border bg-muted/20 p-3">
      <AttachmentHeading
        icon={<WrenchIcon />}
        title={name || t.call}
        type={stringValue(record.type)}
      />
      <AttachmentFields
        rows={[
          [t.identifier, stringValue(record.call_id) || stringValue(record.id)],
          [t.name, name],
        ]}
      />
      {payload !== undefined && <RawJson value={displayValue(payload)} />}
    </div>
  )
}

function StructuredContent({
  record,
  label,
}: {
  record: JsonRecord
  label: string
}) {
  return (
    <div className="flex min-w-0 flex-col gap-2 rounded-md border bg-muted/20 p-3">
      <Badge className="w-fit" variant="outline">
        {label}
      </Badge>
      <RawJson value={displayValue(record)} />
    </div>
  )
}

function AttachmentHeading({
  icon,
  title,
  type,
}: {
  icon: ReactNode
  title: string
  type?: string
}) {
  return (
    <div className="flex min-w-0 flex-wrap items-center gap-2">
      <span className="text-muted-foreground [&_svg]:size-4">{icon}</span>
      <span className="min-w-0 font-mono text-xs font-medium break-all">
        {title}
      </span>
      {type && <Badge variant="outline">{type}</Badge>}
    </div>
  )
}

function AttachmentFields({
  rows,
}: {
  rows: readonly (readonly [string, string | undefined])[]
}) {
  const present = rows.filter(
    ([, value]) => value !== undefined && value !== ""
  )
  if (present.length === 0) return null
  return (
    <dl className="grid gap-x-4 gap-y-2 sm:grid-cols-2">
      {present.map(([label, value]) => (
        <div key={label} className="min-w-0">
          <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
          <dd className="mt-0.5 font-mono text-xs break-all">{value}</dd>
        </div>
      ))}
    </dl>
  )
}

function ToolList({ tools, locale }: { tools: unknown[]; locale: Locale }) {
  if (tools.length === 0)
    return <p className="text-sm leading-6 text-muted-foreground">—</p>
  return (
    <ul className="overflow-hidden rounded-md border">
      {tools.map((tool, index) => (
        <ToolDefinition key={index} tool={tool} index={index} locale={locale} />
      ))}
    </ul>
  )
}

function ToolDefinition({
  tool,
  index,
  locale,
}: {
  tool: unknown
  index: number
  locale: Locale
}) {
  const t = copy[locale]
  const record = recordValue(tool)
  if (!record) {
    return (
      <li className="p-3">
        <RawJson value={displayValue(tool)} />
      </li>
    )
  }
  const type = stringValue(record.type) || t.tool
  const name = stringValue(record.name) || `${t.tool} #${index + 1}`
  const description = stringValue(record.description)
  const parameters = recordValue(record.parameters)
  const nestedTools = Array.isArray(record.tools) ? record.tools : []
  const properties = recordValue(parameters?.properties)
  const required = new Set(
    Array.isArray(parameters?.required)
      ? parameters.required.filter(
          (value): value is string => typeof value === "string"
        )
      : []
  )
  const toolDetails = Object.entries(record).filter(
    ([key]) =>
      !["type", "name", "description", "parameters", "tools"].includes(key)
  )
  return (
    <li className="flex min-w-0 flex-col gap-3 border-b p-3 last:border-b-0">
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <WrenchIcon className="size-4 text-muted-foreground" />
        <code className="min-w-0 text-sm font-medium break-all">{name}</code>
        <Badge variant="secondary">
          {type === "function" ? t.functionTool : type}
        </Badge>
      </div>
      {description && <NaturalLanguageValue value={description} />}
      {properties && Object.keys(properties).length > 0 && (
        <div className="flex min-w-0 flex-col gap-2">
          <h4 className="text-sm font-medium">{t.parameters}</h4>
          <ul className="overflow-hidden rounded-md border">
            {Object.entries(properties).map(([name, schema]) => (
              <ToolParameter
                key={name}
                name={name}
                schema={schema}
                required={required.has(name)}
                locale={locale}
              />
            ))}
          </ul>
        </div>
      )}
      {nestedTools.length > 0 && (
        <div className="flex min-w-0 flex-col gap-2">
          <h4 className="text-sm font-medium">{t.tools}</h4>
          <ToolList tools={nestedTools} locale={locale} />
        </div>
      )}
      {toolDetails.length > 0 && <OptionList rows={toolDetails} />}
    </li>
  )
}

function ToolParameter({
  name,
  schema,
  required,
  locale,
}: {
  name: string
  schema: unknown
  required: boolean
  locale: Locale
}) {
  const t = copy[locale]
  const record = recordValue(schema)
  const type =
    stringValue(record?.type) || displayValue(record?.anyOf ?? record?.oneOf)
  const description = stringValue(record?.description)
  const details = record
    ? Object.entries(record).filter(
        ([key]) => !["type", "description"].includes(key)
      )
    : []
  return (
    <li className="flex min-w-0 flex-col gap-2 border-b p-3 last:border-b-0 sm:flex-row sm:gap-4">
      <code className="min-w-0 shrink-0 text-xs font-medium break-all sm:w-40">
        {name}
      </code>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-mono text-xs text-muted-foreground">
            {type}
          </span>
          <Badge variant="outline">{required ? t.required : t.optional}</Badge>
        </div>
        {description && (
          <p className="mt-1 max-w-[75ch] text-sm leading-6">{description}</p>
        )}
        {details.length > 0 && (
          <div className="mt-2">
            <OptionList rows={details} />
          </div>
        )}
      </div>
    </li>
  )
}

function RawJson({ value, className }: { value: string; className?: string }) {
  return (
    <pre
      className={`max-h-80 overflow-auto rounded-md border bg-muted p-3 font-mono text-xs break-all whitespace-pre-wrap${className ? ` ${className}` : ""}`}
    >
      {value}
    </pre>
  )
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined
}

function safeHttpUrl(value: string | undefined) {
  return typeof value === "string" && /^https?:\/\//i.test(value)
}

function formatBytes(bytes: number, locale: Locale) {
  return (
    new Intl.NumberFormat(locale === "zh" ? "zh-CN" : "en-US", {
      maximumFractionDigits: 1,
    }).format(bytes) + " B"
  )
}
