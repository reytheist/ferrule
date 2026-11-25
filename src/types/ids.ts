import type {
  AccountId,
  AttachmentId,
  CommentId,
  ContactId,
  DraftId,
  IdentityId,
  JobId,
  LabelId,
  MessageId,
  PinnedLinkId,
  SharedTemplateId,
  SharedThreadId,
  StaffMemberId,
  TemplateId,
  ThreadId,
} from "./mail";

// Internally generated IDs are constructed through a verifying guard so no
// bare cast ever produces a branded value.

function makeId<V extends string>(prefix: string, guard: (value: string) => value is V): V {
  const value = `${prefix}_${crypto.randomUUID()}`;
  if (!guard(value)) {
    throw new Error(`generated id "${value}" failed its ${prefix} guard`);
  }
  return value;
}

const isAccountId = (v: string): v is AccountId => v.startsWith("acc_");
const isIdentityId = (v: string): v is IdentityId => v.startsWith("idn_");
const isThreadId = (v: string): v is ThreadId => v.startsWith("thr_");
const isMessageId = (v: string): v is MessageId => v.startsWith("msg_");
const isAttachmentId = (v: string): v is AttachmentId => v.startsWith("att_");
const isDraftId = (v: string): v is DraftId => v.startsWith("drf_");
const isJobId = (v: string): v is JobId => v.startsWith("job_");
const isTemplateId = (v: string): v is TemplateId => v.startsWith("tpl_");
const isLabelId = (v: string): v is LabelId => v.startsWith("lbl_");
const isStaffMemberId = (v: string): v is StaffMemberId => v.startsWith("stf_");
const isSharedThreadId = (v: string): v is SharedThreadId => v.startsWith("sht_");
const isCommentId = (v: string): v is CommentId => v.startsWith("cmt_");
const isSharedTemplateId = (v: string): v is SharedTemplateId => v.startsWith("stp_");
const isContactId = (v: string): v is ContactId => v.startsWith("cnt_");
const isPinnedLinkId = (v: string): v is PinnedLinkId => v.startsWith("lnk_");

export const newAccountId = (): AccountId => makeId("acc", isAccountId);
export const newIdentityId = (): IdentityId => makeId("idn", isIdentityId);
export const newThreadId = (): ThreadId => makeId("thr", isThreadId);
export const newMessageId = (): MessageId => makeId("msg", isMessageId);
export const newAttachmentId = (): AttachmentId => makeId("att", isAttachmentId);
export const newDraftId = (): DraftId => makeId("drf", isDraftId);
export const newJobId = (): JobId => makeId("job", isJobId);
export const newTemplateId = (): TemplateId => makeId("tpl", isTemplateId);
export const newLabelId = (): LabelId => makeId("lbl", isLabelId);
export const newStaffMemberId = (): StaffMemberId => makeId("stf", isStaffMemberId);
export const newSharedThreadId = (): SharedThreadId => makeId("sht", isSharedThreadId);
export const newCommentId = (): CommentId => makeId("cmt", isCommentId);
export const newSharedTemplateId = (): SharedTemplateId => makeId("stp", isSharedTemplateId);
export const newContactId = (): ContactId => makeId("cnt", isContactId);
export const newPinnedLinkId = (): PinnedLinkId => makeId("lnk", isPinnedLinkId);
