/**
 * 富文本（description）辅助工具
 *
 * description 由 Tiptap 编辑器产出 HTML，经后端 ammonia 白名单净化后持久化。
 * 前端仅需：判断内容是否为空（stripHtml）。
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
