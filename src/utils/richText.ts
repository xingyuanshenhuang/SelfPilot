/**
 * 富文本（description）辅助工具
 *
 * description 由 Tiptap 编辑器产出 HTML，经后端 ammonia 白名单净化后持久化。
 * 前端仅需：① 判断内容是否为空（stripHtml）；② 保存时决定传 HTML 还是 null。
 */

/** 去除 HTML 标签，返回纯文本（状态机实现，不引外部依赖，不做实体解码） */
export function stripHtml(html: string): string {
  let out = "";
  let inTag = false;
  for (const c of html) {
    if (c === "<") {
      inTag = true;
    } else if (c === ">") {
      inTag = false;
    } else if (!inTag) {
      out += c;
    }
  }
  return out;
}

/**
 * 保存归一化：内容（去标签后）非空白则返回原始 HTML，否则返回 null（清除）。
 * 后端收到非空 HTML 会再净化一次，前端不做净化。
 */
export function descToSend(html: string): string | null {
  return stripHtml(html).trim() ? html : null;
}
