#include "pdbg_shim.h"

#include <mupdf/fitz.h>
#include <mupdf/pdf.h>
#include <mupdf/pdf/javascript.h>

#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct pdbg_context {
    fz_context *ctx;
    fz_locks_context lock_ctx;
    pthread_mutex_t locks[FZ_LOCK_MAX];
    int locks_initialized;
};

struct pdbg_doc {
    uint64_t document_id;
    pdbg_context *owner;
    fz_document *fz_doc;
    pdf_document *pdf_doc;
    char *file_path;
    int encrypted;
    int needs_password;
    int authenticated;
    int safe_mode;
    int javascript_disabled;
    int repaired_or_damaged;
};

struct pdbg_buffer {
    uint8_t *data;
    size_t len;
    uint64_t total_size;
    int truncated;
    pdbg_diagnostic_list *diagnostics;
};

struct pdbg_image {
    uint32_t width;
    uint32_t height;
    size_t stride;
    uint8_t *pixels;
    pdbg_diagnostic_list *diagnostics;
};

struct pdbg_node_list {
    size_t len;
    int has_total;
    size_t total;
    pdbg_dict_entry *items;
};

struct pdbg_diagnostic_list {
    size_t len;
    pdbg_diagnostic *items;
};

struct pdbg_text_page {
    size_t len;
    pdbg_text_span *spans;
};

struct pdbg_cancel_token {
    int cancelled;
};

static atomic_uint_fast64_t next_document_id = 1;

static void set_error(pdbg_error *err, pdbg_status status, const char *message)
{
    if (!err)
        return;

    err->status = status;
    err->mupdf_code = 0;
    if (!message)
        message = "";
    snprintf(err->message, sizeof(err->message), "%s", message);
}

static pdbg_status set_mupdf_error(fz_context *ctx, pdbg_error *err)
{
    int code = fz_caught(ctx);
    pdbg_status status = PDBG_ERROR_GENERIC;

    switch (code) {
    case FZ_ERROR_FORMAT:
    case FZ_ERROR_SYNTAX:
    case FZ_ERROR_REPAIRED:
        status = PDBG_ERROR_FORMAT;
        break;
    case FZ_ERROR_UNSUPPORTED:
        status = PDBG_ERROR_UNSUPPORTED;
        break;
    case FZ_ERROR_ABORT:
        status = PDBG_ERROR_CANCELLED;
        break;
    case FZ_ERROR_LIMIT:
        status = PDBG_ERROR_LIMIT;
        break;
    default:
        status = PDBG_ERROR_GENERIC;
        break;
    }

    if (err) {
        err->status = status;
        err->mupdf_code = code;
        snprintf(err->message, sizeof(err->message), "%s", fz_caught_message(ctx));
    }
    return status;
}

static pdbg_status unsupported(pdbg_error *err, const char *operation)
{
    char message[256];
    snprintf(message, sizeof(message), "%s is not implemented by the M1 real-mupdf shim", operation);
    set_error(err, PDBG_ERROR_UNSUPPORTED, message);
    return PDBG_ERROR_UNSUPPORTED;
}

static char *copy_string(const char *value)
{
    size_t len = value ? strlen(value) : 0;
    char *out = (char *)malloc(len + 1);
    if (!out)
        return NULL;
    if (len)
        memcpy(out, value, len);
    out[len] = '\0';
    return out;
}

static void lock_mutex(void *user, int lock)
{
    pdbg_context *ctx = (pdbg_context *)user;
    if (!ctx || lock < 0 || lock >= FZ_LOCK_MAX)
        return;
    pthread_mutex_lock(&ctx->locks[lock]);
}

static void unlock_mutex(void *user, int lock)
{
    pdbg_context *ctx = (pdbg_context *)user;
    if (!ctx || lock < 0 || lock >= FZ_LOCK_MAX)
        return;
    pthread_mutex_unlock(&ctx->locks[lock]);
}

static void destroy_locks(pdbg_context *ctx)
{
    if (!ctx)
        return;
    for (int i = 0; i < ctx->locks_initialized; i++)
        pthread_mutex_destroy(&ctx->locks[i]);
    ctx->locks_initialized = 0;
}

static pdbg_diagnostic_list *make_diag_list(pdbg_diagnostic_code code, const char *message)
{
    pdbg_diagnostic_list *list = (pdbg_diagnostic_list *)calloc(1, sizeof(pdbg_diagnostic_list));
    if (!list)
        return NULL;

    list->items = (pdbg_diagnostic *)calloc(1, sizeof(pdbg_diagnostic));
    if (!list->items) {
        free(list);
        return NULL;
    }

    list->len = 1;
    list->items[0].severity = PDBG_DIAG_WARNING;
    list->items[0].code = code;
    list->items[0].message = copy_string(message);
    return list;
}

static void free_diag_list(pdbg_diagnostic_list *list)
{
    if (!list)
        return;
    for (size_t i = 0; i < list->len; i++)
        free(list->items[i].message);
    free(list->items);
    free(list);
}

static void free_stream_summary(pdbg_stream_summary *stream)
{
    if (!stream)
        return;
    for (size_t i = 0; i < stream->filter_count; i++)
        free(stream->filters[i]);
    free(stream->filters);
    memset(stream, 0, sizeof(*stream));
}

static void free_value(pdbg_object_value *value)
{
    if (!value)
        return;
    free(value->name_value);
    free(value->bytes);
    free(value->decoded_text);
    memset(value, 0, sizeof(*value));
}

static void add_metadata_pair(
    fz_context *ctx,
    fz_document *doc,
    pdbg_string_pair *pairs,
    size_t *len,
    const char *key,
    const char *label)
{
    char value[512];
    int written = fz_lookup_metadata(ctx, doc, key, value, sizeof(value));
    if (written <= 0 || value[0] == '\0')
        return;

    pairs[*len].key = copy_string(label);
    pairs[*len].value = copy_string(value);
    if (pairs[*len].key && pairs[*len].value)
        *len += 1;
    else {
        free(pairs[*len].key);
        free(pairs[*len].value);
        memset(&pairs[*len], 0, sizeof(pairs[*len]));
    }
}

static void fill_metadata(fz_context *ctx, fz_document *doc, pdbg_document_summary_out *out)
{
    const size_t capacity = 5;
    pdbg_string_pair *pairs = (pdbg_string_pair *)calloc(capacity, sizeof(pdbg_string_pair));
    if (!pairs)
        return;

    size_t len = 0;
    add_metadata_pair(ctx, doc, pairs, &len, FZ_META_INFO_TITLE, "Title");
    add_metadata_pair(ctx, doc, pairs, &len, FZ_META_INFO_AUTHOR, "Author");
    add_metadata_pair(ctx, doc, pairs, &len, FZ_META_INFO_CREATOR, "Creator");
    add_metadata_pair(ctx, doc, pairs, &len, FZ_META_INFO_PRODUCER, "Producer");
    add_metadata_pair(ctx, doc, pairs, &len, FZ_META_ENCRYPTION, "Encryption");

    if (len == 0) {
        free(pairs);
        return;
    }
    out->metadata = pairs;
    out->metadata_len = len;
}

static int document_is_encrypted(fz_context *ctx, fz_document *doc)
{
    char encryption[128];
    int written = fz_lookup_metadata(ctx, doc, FZ_META_ENCRYPTION, encryption, sizeof(encryption));
    return written > 0 && strcmp(encryption, "None") != 0;
}

pdbg_status pdbg_context_new(pdbg_context **out, pdbg_error *err)
{
    if (!out) {
        set_error(err, PDBG_ERROR_GENERIC, "out parameter is null");
        return PDBG_ERROR_GENERIC;
    }

    pdbg_context *ctx = (pdbg_context *)calloc(1, sizeof(pdbg_context));
    if (!ctx) {
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    for (int i = 0; i < FZ_LOCK_MAX; i++) {
        if (pthread_mutex_init(&ctx->locks[i], NULL) != 0) {
            destroy_locks(ctx);
            free(ctx);
            set_error(err, PDBG_ERROR_GENERIC, "failed to initialize MuPDF lock");
            return PDBG_ERROR_GENERIC;
        }
        ctx->locks_initialized += 1;
    }

    ctx->lock_ctx.user = ctx;
    ctx->lock_ctx.lock = lock_mutex;
    ctx->lock_ctx.unlock = unlock_mutex;
    ctx->ctx = fz_new_context(NULL, &ctx->lock_ctx, FZ_STORE_DEFAULT);
    if (!ctx->ctx) {
        destroy_locks(ctx);
        free(ctx);
        set_error(err, PDBG_ERROR_OOM, "failed to create MuPDF context");
        return PDBG_ERROR_OOM;
    }

    pdbg_status status = PDBG_OK;
    fz_var(status);
    fz_try(ctx->ctx)
    {
        fz_register_document_handlers(ctx->ctx);
    }
    fz_catch(ctx->ctx)
    {
        status = set_mupdf_error(ctx->ctx, err);
    }

    if (status != PDBG_OK) {
        fz_drop_context(ctx->ctx);
        destroy_locks(ctx);
        free(ctx);
        return status;
    }

    *out = ctx;
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

void pdbg_context_drop(pdbg_context *ctx)
{
    if (!ctx)
        return;
    if (ctx->ctx)
        fz_drop_context(ctx->ctx);
    destroy_locks(ctx);
    free(ctx);
}

pdbg_status pdbg_cancel_token_new(pdbg_cancel_token **out, pdbg_error *err)
{
    if (!out) {
        set_error(err, PDBG_ERROR_GENERIC, "out parameter is null");
        return PDBG_ERROR_GENERIC;
    }

    pdbg_cancel_token *token = (pdbg_cancel_token *)calloc(1, sizeof(pdbg_cancel_token));
    if (!token) {
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    *out = token;
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

void pdbg_cancel_token_cancel(pdbg_cancel_token *token)
{
    if (token)
        token->cancelled = 1;
}

void pdbg_cancel_token_drop(pdbg_cancel_token *token)
{
    free(token);
}

pdbg_status pdbg_document_open(
    pdbg_context *ctx,
    const char *path,
    const char *password,
    const pdbg_open_options *options,
    pdbg_doc **out,
    pdbg_error *err)
{
    if (!ctx || !path || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid open arguments");
        return PDBG_ERROR_GENERIC;
    }
    *out = NULL;

    pdbg_doc *doc = (pdbg_doc *)calloc(1, sizeof(pdbg_doc));
    if (!doc) {
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    doc->owner = ctx;
    doc->document_id = (uint64_t)atomic_fetch_add_explicit(&next_document_id, 1, memory_order_relaxed);
    doc->file_path = copy_string(path);
    doc->safe_mode = options ? options->safe_mode : 1;
    doc->javascript_disabled = options ? options->disable_javascript : 1;
    if (!doc->file_path) {
        free(doc);
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    fz_document *opened = NULL;
    pdf_document *pdf = NULL;
    pdbg_status status = PDBG_OK;
    fz_var(opened);
    fz_var(pdf);
    fz_var(status);

    fz_try(ctx->ctx)
    {
        opened = fz_open_document(ctx->ctx, path);
        pdf = pdf_specifics(ctx->ctx, opened);
        if (!pdf) {
            status = PDBG_ERROR_UNSUPPORTED;
            set_error(err, status, "opened document is not a PDF");
        } else {
            if (doc->safe_mode || doc->javascript_disabled)
                pdf_disable_js(ctx->ctx, pdf);

            doc->encrypted = document_is_encrypted(ctx->ctx, opened);
            doc->needs_password = fz_needs_password(ctx->ctx, opened);
            doc->authenticated = doc->needs_password ? 0 : 1;

            if (doc->needs_password && password && password[0] != '\0') {
                if (fz_authenticate_password(ctx->ctx, opened, password)) {
                    doc->needs_password = 0;
                    doc->authenticated = 1;
                } else {
                    status = PDBG_ERROR_PASSWORD;
                    set_error(err, status, "password authentication failed");
                }
            }

            if (status == PDBG_OK && doc->authenticated)
                doc->repaired_or_damaged = pdf_was_repaired(ctx->ctx, pdf);
        }
    }
    fz_catch(ctx->ctx)
    {
        status = set_mupdf_error(ctx->ctx, err);
    }

    if (status != PDBG_OK) {
        if (opened)
            fz_drop_document(ctx->ctx, opened);
        free(doc->file_path);
        free(doc);
        return status;
    }

    doc->fz_doc = opened;
    doc->pdf_doc = pdf;
    *out = doc;
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

pdbg_status pdbg_document_open_fd(
    pdbg_context *ctx,
    int fd,
    const char *display_path,
    const char *password,
    const pdbg_open_options *options,
    pdbg_doc **out,
    pdbg_error *err)
{
    (void)ctx;
    (void)fd;
    (void)display_path;
    (void)password;
    (void)options;
    if (out)
        *out = NULL;
    return unsupported(err, "pdbg_document_open_fd");
}

void pdbg_document_drop(pdbg_doc *doc)
{
    if (!doc)
        return;
    if (doc->owner && doc->owner->ctx && doc->fz_doc)
        fz_drop_document(doc->owner->ctx, doc->fz_doc);
    free(doc->file_path);
    free(doc);
}

pdbg_status pdbg_document_summary(pdbg_doc *doc, pdbg_document_summary_out *out, pdbg_error *err)
{
    if (!doc || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid summary arguments");
        return PDBG_ERROR_GENERIC;
    }

    memset(out, 0, sizeof(*out));
    out->document_id = doc->document_id;
    out->file_path = copy_string(doc->file_path);
    if (doc->file_path && !out->file_path) {
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }
    out->encrypted = doc->encrypted;
    out->needs_password = doc->needs_password;
    out->safe_mode = doc->safe_mode;
    out->javascript_disabled = doc->safe_mode || doc->javascript_disabled;
    out->repaired_or_damaged = doc->repaired_or_damaged;

    if (!doc->authenticated) {
        set_error(err, PDBG_OK, "");
        return PDBG_OK;
    }

    fz_context *ctx = doc->owner->ctx;
    pdbg_status status = PDBG_OK;
    fz_var(status);

    fz_try(ctx)
    {
        int version = pdf_version(ctx, doc->pdf_doc);
        char version_buf[16];
        snprintf(version_buf, sizeof(version_buf), "%d.%d", version / 10, version % 10);

        out->pdf_version = copy_string(version_buf);
        if (!out->pdf_version) {
            status = PDBG_ERROR_OOM;
            set_error(err, status, "out of memory");
        } else {
            out->page_count = (size_t)fz_count_pages(ctx, doc->fz_doc);
            out->xref_size = (size_t)pdf_xref_len(ctx, doc->pdf_doc);
            out->parsed_object_count = (size_t)pdf_count_objects(ctx, doc->pdf_doc);
            out->has_parsed_object_count = 1;
            out->permissions.print = pdf_has_permission(ctx, doc->pdf_doc, FZ_PERMISSION_PRINT);
            out->permissions.modify = pdf_has_permission(ctx, doc->pdf_doc, FZ_PERMISSION_EDIT);
            out->permissions.copy = pdf_has_permission(ctx, doc->pdf_doc, FZ_PERMISSION_COPY);
            out->permissions.annotate = pdf_has_permission(ctx, doc->pdf_doc, FZ_PERMISSION_ANNOTATE);
            out->permissions.fill_forms = pdf_has_permission(ctx, doc->pdf_doc, FZ_PERMISSION_FORM);
            out->permissions.extract_accessibility =
                pdf_has_permission(ctx, doc->pdf_doc, FZ_PERMISSION_ACCESSIBILITY);
            out->permissions.assemble = pdf_has_permission(ctx, doc->pdf_doc, FZ_PERMISSION_ASSEMBLE);
            out->permissions.high_quality_print = pdf_has_permission(ctx, doc->pdf_doc, FZ_PERMISSION_PRINT_HQ);
            fill_metadata(ctx, doc->fz_doc, out);

            if (pdf_was_repaired(ctx, doc->pdf_doc)) {
                out->repaired_or_damaged = 1;
                out->diagnostics =
                    make_diag_list(PDBG_DIAG_REPAIR_WARNING, "MuPDF repaired the document on open");
            }
        }
    }
    fz_catch(ctx)
    {
        status = set_mupdf_error(ctx, err);
    }

    if (status != PDBG_OK) {
        pdbg_document_summary_out_drop(out);
        return status;
    }

    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

pdbg_status pdbg_node_children(
    pdbg_doc *doc,
    const pdbg_node_id *node,
    size_t offset,
    size_t limit,
    pdbg_node_list **out,
    pdbg_error *err)
{
    (void)doc;
    (void)node;
    (void)offset;
    (void)limit;
    if (out)
        *out = NULL;
    return unsupported(err, "pdbg_node_children");
}

pdbg_status pdbg_object_detail(
    pdbg_doc *doc,
    const pdbg_node_id *node,
    pdbg_object_detail_out *out,
    pdbg_error *err)
{
    (void)doc;
    (void)node;
    if (out)
        memset(out, 0, sizeof(*out));
    return unsupported(err, "pdbg_object_detail");
}

pdbg_status pdbg_stream_load(
    pdbg_doc *doc,
    pdbg_object_id object,
    int decoded,
    uint64_t offset,
    size_t limit,
    pdbg_cancel_token *cancel,
    pdbg_buffer **out,
    pdbg_error *err)
{
    (void)doc;
    (void)object;
    (void)decoded;
    (void)offset;
    (void)limit;
    (void)cancel;
    if (out)
        *out = NULL;
    return unsupported(err, "pdbg_stream_load");
}

pdbg_status pdbg_page_render(
    pdbg_doc *doc,
    uint32_t page_index,
    const pdbg_render_options *options,
    pdbg_cancel_token *cancel,
    pdbg_image **out,
    pdbg_error *err)
{
    (void)doc;
    (void)page_index;
    (void)options;
    (void)cancel;
    if (out)
        *out = NULL;
    return unsupported(err, "pdbg_page_render");
}

pdbg_status pdbg_page_extract_text(
    pdbg_doc *doc,
    uint32_t page_index,
    const pdbg_text_options *options,
    pdbg_cancel_token *cancel,
    pdbg_text_page **out,
    pdbg_error *err)
{
    (void)doc;
    (void)page_index;
    (void)options;
    (void)cancel;
    if (out)
        *out = NULL;
    return unsupported(err, "pdbg_page_extract_text");
}

void pdbg_buffer_drop(pdbg_buffer *buffer)
{
    if (!buffer)
        return;
    free(buffer->data);
    free_diag_list(buffer->diagnostics);
    free(buffer);
}

void pdbg_image_drop(pdbg_image *image)
{
    if (!image)
        return;
    free(image->pixels);
    free_diag_list(image->diagnostics);
    free(image);
}

void pdbg_node_list_drop(pdbg_node_list *list)
{
    if (!list)
        return;
    for (size_t i = 0; i < list->len; i++) {
        free(list->items[i].key);
        free(list->items[i].label);
        free(list->items[i].preview);
        free_diag_list(list->items[i].diagnostics);
    }
    free(list->items);
    free(list);
}

void pdbg_text_page_drop(pdbg_text_page *text)
{
    if (!text)
        return;
    for (size_t i = 0; i < text->len; i++)
        free(text->spans[i].text);
    free(text->spans);
    free(text);
}

void pdbg_document_summary_out_drop(pdbg_document_summary_out *out)
{
    if (!out)
        return;
    free(out->file_path);
    free(out->file_hash);
    free(out->pdf_version);
    for (size_t i = 0; i < out->metadata_len; i++) {
        free(out->metadata[i].key);
        free(out->metadata[i].value);
    }
    free(out->metadata);
    free_diag_list(out->diagnostics);
    memset(out, 0, sizeof(*out));
}

void pdbg_object_detail_out_drop(pdbg_object_detail_out *out)
{
    if (!out)
        return;
    free(out->label);
    free(out->preview);
    free_value(&out->value);
    pdbg_node_list_drop(out->children);
    pdbg_node_list_drop(out->dictionary_entries);
    free_stream_summary(&out->stream);
    free_diag_list(out->diagnostics);
    memset(out, 0, sizeof(*out));
}

const uint8_t *pdbg_buffer_data(const pdbg_buffer *buffer)
{
    return buffer ? buffer->data : NULL;
}

size_t pdbg_buffer_len(const pdbg_buffer *buffer)
{
    return buffer ? buffer->len : 0;
}

uint64_t pdbg_buffer_total_size_hint(const pdbg_buffer *buffer)
{
    return buffer ? buffer->total_size : 0;
}

int pdbg_buffer_truncated(const pdbg_buffer *buffer)
{
    return buffer ? buffer->truncated : 0;
}

size_t pdbg_buffer_diagnostic_count(const pdbg_buffer *buffer)
{
    return buffer && buffer->diagnostics ? buffer->diagnostics->len : 0;
}

pdbg_status pdbg_buffer_diagnostic_get(
    const pdbg_buffer *buffer,
    size_t index,
    pdbg_diagnostic *out,
    pdbg_error *err)
{
    if (!buffer || !buffer->diagnostics || !out || index >= buffer->diagnostics->len) {
        set_error(err, PDBG_ERROR_GENERIC, "diagnostic index out of range");
        return PDBG_ERROR_GENERIC;
    }
    *out = buffer->diagnostics->items[index];
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

uint32_t pdbg_image_width(const pdbg_image *image)
{
    return image ? image->width : 0;
}

uint32_t pdbg_image_height(const pdbg_image *image)
{
    return image ? image->height : 0;
}

size_t pdbg_image_stride(const pdbg_image *image)
{
    return image ? image->stride : 0;
}

const uint8_t *pdbg_image_rgba_pixels(const pdbg_image *image)
{
    return image ? image->pixels : NULL;
}

size_t pdbg_image_diagnostic_count(const pdbg_image *image)
{
    return image && image->diagnostics ? image->diagnostics->len : 0;
}

pdbg_status pdbg_image_diagnostic_get(
    const pdbg_image *image,
    size_t index,
    pdbg_diagnostic *out,
    pdbg_error *err)
{
    if (!image || !image->diagnostics || !out || index >= image->diagnostics->len) {
        set_error(err, PDBG_ERROR_GENERIC, "diagnostic index out of range");
        return PDBG_ERROR_GENERIC;
    }
    *out = image->diagnostics->items[index];
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

size_t pdbg_node_list_len(const pdbg_node_list *list)
{
    return list ? list->len : 0;
}

int pdbg_node_list_has_total_count(const pdbg_node_list *list)
{
    return list ? list->has_total : 0;
}

size_t pdbg_node_list_total_count(const pdbg_node_list *list)
{
    return list ? list->total : 0;
}

pdbg_status pdbg_node_list_get(
    const pdbg_node_list *list,
    size_t index,
    pdbg_dict_entry *out,
    pdbg_error *err)
{
    if (!list || !out || index >= list->len) {
        set_error(err, PDBG_ERROR_GENERIC, "node list index out of range");
        return PDBG_ERROR_GENERIC;
    }
    *out = list->items[index];
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

size_t pdbg_diagnostic_list_len(const pdbg_diagnostic_list *list)
{
    return list ? list->len : 0;
}

pdbg_status pdbg_diagnostic_list_get(
    const pdbg_diagnostic_list *list,
    size_t index,
    pdbg_diagnostic *out,
    pdbg_error *err)
{
    if (!list || !out || index >= list->len) {
        set_error(err, PDBG_ERROR_GENERIC, "diagnostic index out of range");
        return PDBG_ERROR_GENERIC;
    }
    *out = list->items[index];
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

size_t pdbg_text_page_span_count(const pdbg_text_page *text)
{
    return text ? text->len : 0;
}

pdbg_status pdbg_text_page_span_get(
    const pdbg_text_page *text,
    size_t index,
    pdbg_text_span *out,
    pdbg_error *err)
{
    if (!text || !out || index >= text->len) {
        set_error(err, PDBG_ERROR_GENERIC, "text span index out of range");
        return PDBG_ERROR_GENERIC;
    }
    *out = text->spans[index];
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

pdbg_status pdbg_test_invoke_callback(
    pdbg_test_callback callback,
    void *user,
    pdbg_error *err)
{
    if (!callback) {
        set_error(err, PDBG_ERROR_GENERIC, "callback is null");
        return PDBG_ERROR_GENERIC;
    }
    return callback(user, err);
}

int pdbg_test_document_owned_fd(const pdbg_doc *doc)
{
    (void)doc;
    return -1;
}

int pdbg_test_fd_is_open(int fd)
{
    (void)fd;
    return 0;
}
