<script lang="ts">
  import type { ConversationNode, ConversationImage } from "$lib/types/conversation";
  import ActionIconButton from "$lib/components/shared/ui/ActionIconButton.svelte";
  import SkillEditable from "$lib/components/shared/widgets/SkillEditable.svelte";
  import AttachmentRow from "./components/AttachmentRow.svelte";
  import BubbleEditField from "./components/BubbleEditField.svelte";
  import BubbleActionsFooter from "./components/BubbleActionsFooter.svelte";
  import { Trash2, Pencil, Copy, Check, ArrowUp } from "lucide-svelte";
  import { handleEditablePaste } from "$lib/utils/paste";
  import { highlightSkills } from "$lib/utils/skillHighlight";
  import { openImagePreview, openTextPreview } from "$lib/services/windowPreviews";

  let {
    node,
    showDelete = false,
    classifyToken,
    onContentChange,
    onDelete,
    onRegenerate,
    onRemoveTextAttachment,
    onRemoveImage,
    onAddTextAttachment,
    onAddImage,
  }: {
    node: ConversationNode;
    showDelete: boolean;
    classifyToken: (token: string, finished: boolean) => string | null;
    onContentChange: (content: string) => void | Promise<void>;
    onDelete: (nodeId: string) => void;
    onRegenerate: () => void;
    onRemoveTextAttachment: (index: number) => void;
    onRemoveImage: (index: number) => void;
    onAddTextAttachment: (text: string) => void;
    onAddImage: (data: string, mediaType: string) => void;
  } = $props();

  function formatUserContent(content: string): string {
    return highlightSkills(content, classifyToken, "\n");
  }

  let editMode = $state(false);
  let editText = $state("");
  let skillEditable: ReturnType<typeof SkillEditable> | undefined = $state();

  function toggleEditMode() {
    editMode = !editMode;
    if (editMode) {
      editText = node.content;
      requestAnimationFrame(() => {
        if (skillEditable) {
          skillEditable.setTextAndHighlight(editText);
          skillEditable.focus();
          skillEditable.restoreCursor(editText.length);
        }
      });
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submitEdit();
    }
  }

  async function submitEdit() {
    await onContentChange(editText);
    editMode = false;
    onRegenerate();
  }

  async function handlePaste(e: ClipboardEvent) {
    await handleEditablePaste(e, {
      onTextAttachment: onAddTextAttachment,
      onImage: (img) => onAddImage(img.data, img.media_type),
    });
  }

  async function copyContent() {
    await navigator.clipboard.writeText(node.content);
  }

  function handleOpenImage(image: ConversationImage) {
    openImagePreview(image.data, image.media_type);
  }
</script>

<div class="user-message-wrapper" class:editing={editMode}>
  <div class="user-bubble" class:editing={editMode}>
    <div class="bubble-body">
      {#if editMode}
        <BubbleEditField variant="user">
          <AttachmentRow
            textAttachments={node.text_attachments}
            images={node.images}
            onRemoveText={onRemoveTextAttachment}
            onRemoveImage={onRemoveImage}
            onOpenText={openTextPreview}
            onOpenImage={handleOpenImage}
          />
          <SkillEditable
            bind:this={skillEditable}
            bind:text={editText}
            editableClass="bubble-editable"
            onkeydown={handleKeydown}
            onpaste={handlePaste}
          />
        </BubbleEditField>
      {:else}
        <AttachmentRow
          textAttachments={node.text_attachments}
          images={node.images}
          readonly={true}
          onOpenText={openTextPreview}
          onOpenImage={handleOpenImage}
        />
        <div class="bubble-text">{@html formatUserContent(node.content)}</div>
      {/if}
    </div>
  </div>
  <BubbleActionsFooter visible={editMode}>
    <ActionIconButton
      icon={Copy}
      confirmIcon={Check}
      onclick={copyContent}
      label="Copy text"
    />
    <ActionIconButton
      icon={Pencil}
      active={editMode}
      onclick={toggleEditMode}
      label={editMode ? "View" : "Edit"}
    />
    {#if editMode}
      <ActionIconButton
        icon={ArrowUp}
        onclick={submitEdit}
        label="Send"
        title="Send (Enter)"
      />
    {/if}
    {#if showDelete}
      <ActionIconButton
        icon={Trash2}
        variant="danger"
        onclick={() => onDelete(node.node_id)}
        label="Delete"
      />
    {/if}
  </BubbleActionsFooter>
</div>

<style>
  .user-message-wrapper {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    max-width: 80%;
    margin-left: auto;
  }

  .user-message-wrapper.editing {
    max-width: 100%;
    width: 100%;
    margin-left: var(--space-0);
    align-items: stretch;
  }

  .user-bubble {
    padding: var(--space-7) var(--space-10);
    background: var(--user-bubble-bg);
    border-radius: var(--radius-2xl);
    box-sizing: border-box;
    user-select: none;
    -webkit-user-select: none;
  }

  .bubble-text {
    font-size: var(--font-size-lg);
    line-height: var(--line-height-normal);
    color: var(--text-primary);
    white-space: pre-wrap;
    word-wrap: break-word;
    user-select: text;
    -webkit-user-select: text;
  }

  .bubble-text :global(.skill-badge) {
    display: inline-block;
    padding: 0 6px;
    margin: 0 1px;
    border-radius: 4px;
    background: var(--surface-overlay);
    color: var(--accent-vivid);
    font-family: var(--font-mono);
    font-size: 0.92em;
    font-weight: 500;
    line-height: 1.5;
    vertical-align: baseline;
  }

  .user-bubble.editing :global(.bubble-editable) {
    font-size: var(--font-size-lg);
    line-height: var(--line-height-normal);
    max-height: 40vh;
    padding: var(--space-2) var(--space-0) var(--space-4);
  }

  .user-bubble.editing :global(.autocomplete-dropdown) {
    bottom: auto;
    top: 100%;
    margin-bottom: var(--space-0);
    margin-top: var(--space-2);
  }
</style>
