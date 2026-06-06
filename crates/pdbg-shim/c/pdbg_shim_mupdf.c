#include "pdbg_shim.h"

#include <mupdf/fitz.h>
#include <mupdf/pdf.h>
#include <mupdf/pdf/javascript.h>

#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

struct pdbg_context {
    fz_context *ctx;
    fz_locks_context lock_ctx;
    pthread_mutex_t locks[FZ_LOCK_MAX];
    pthread_mutex_t open_mutex;
    int locks_initialized;
    int open_mutex_initialized;
};

struct pdbg_doc {
    uint64_t document_id;
    pdbg_context *owner;
    fz_context *ctx;
    fz_document *fz_doc;
    pdf_document *pdf_doc;
    FILE *owned_file;
    char *file_path;
    int encrypted;
    int needs_password;
    int authenticated;
    int safe_mode;
    int javascript_disabled;
    int allow_external_references;
    int repaired_or_damaged;
    pdbg_repair_policy repair_policy;
    uint64_t max_store_bytes;
    uint64_t max_decoded_stream_bytes;
    uint32_t max_filter_expansion_ratio;
    uint32_t max_object_depth;
    uint64_t next_path_token;
    struct pdbg_path_binding *paths;
    size_t path_len;
    size_t path_cap;
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
    atomic_int cancelled;
    pthread_mutex_t mutex;
    int mutex_initialized;
    /* Protected by mutex. The pointed fz_cookie objects are stack-owned by active
       MuPDF calls and are unlinked before returning. The abort field itself is
       MuPDF's documented asynchronous cancellation channel, so controller writes
       race with MuPDF reads by design; TSan runs suppress only that field access. */
    struct pdbg_active_cookie *active_cookies;
};

struct pdbg_active_cookie {
    fz_cookie *cookie;
    struct pdbg_active_cookie *next;
};

typedef struct pdbg_cancel_cookie_scope {
    pdbg_cancel_token *token;
    fz_cookie cookie;
    struct pdbg_active_cookie link;
    int registered;
} pdbg_cancel_cookie_scope;

static atomic_uint_fast64_t next_document_id = 1;

struct pdbg_path_binding {
    uint64_t token;
    pdf_obj *obj;
};

#define PDBG_DEFAULT_MAX_STORE_BYTES (256ULL * 1024ULL * 1024ULL)
#define PDBG_DEFAULT_MAX_DECODED_STREAM_BYTES (64ULL * 1024ULL * 1024ULL)
#define PDBG_DEFAULT_MAX_FILTER_EXPANSION_RATIO 64U
#define PDBG_DEFAULT_MAX_OBJECT_DEPTH 128U
#define PDBG_DEFAULT_MAX_TEXT_CHARS 1000000ULL
#define PDBG_DEFAULT_MAX_TEXT_BLOCKS 100000ULL

static void apply_open_options(pdbg_doc *doc, const pdbg_open_options *options)
{
    doc->safe_mode = options ? options->safe_mode : 1;
    doc->javascript_disabled = options ? options->disable_javascript : 1;
    doc->allow_external_references = options ? options->allow_external_references : 0;
    doc->repair_policy = options ? options->repair_policy : PDBG_REPAIR_DEFAULT;
    doc->max_store_bytes =
        options && options->max_store_bytes ? options->max_store_bytes : PDBG_DEFAULT_MAX_STORE_BYTES;
    doc->max_decoded_stream_bytes =
        options && options->max_decoded_stream_bytes ? options->max_decoded_stream_bytes
                                                     : PDBG_DEFAULT_MAX_DECODED_STREAM_BYTES;
    doc->max_filter_expansion_ratio =
        options && options->max_filter_expansion_ratio ? options->max_filter_expansion_ratio
                                                       : PDBG_DEFAULT_MAX_FILTER_EXPANSION_RATIO;
    doc->max_object_depth =
        options && options->max_object_depth ? options->max_object_depth : PDBG_DEFAULT_MAX_OBJECT_DEPTH;
}

static int repair_is_forbidden(const pdbg_doc *doc)
{
    return doc && doc->repair_policy == PDBG_REPAIR_NEVER && doc->repaired_or_damaged;
}

static void free_diag_list(pdbg_diagnostic_list *list);

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

static void ignore_mupdf_message(void *user, const char *message)
{
    (void)user;
    (void)message;
}

static void destroy_locks(pdbg_context *ctx)
{
    if (!ctx)
        return;
    for (int i = 0; i < ctx->locks_initialized; i++)
        pthread_mutex_destroy(&ctx->locks[i]);
    ctx->locks_initialized = 0;
    if (ctx->open_mutex_initialized) {
        pthread_mutex_destroy(&ctx->open_mutex);
        ctx->open_mutex_initialized = 0;
    }
}

static int clone_doc_context(pdbg_context *owner, pdbg_doc *doc, pdbg_error *err)
{
    pthread_mutex_lock(&owner->open_mutex);
    doc->ctx = fz_clone_context(owner->ctx);
    pthread_mutex_unlock(&owner->open_mutex);
    if (!doc->ctx) {
        set_error(err, PDBG_ERROR_OOM, "failed to clone MuPDF context");
        return 0;
    }
    fz_set_error_callback(doc->ctx, ignore_mupdf_message, NULL);
    fz_set_warning_callback(doc->ctx, ignore_mupdf_message, NULL);
    return 1;
}

static void drop_doc_context(pdbg_doc *doc)
{
    if (!doc || !doc->ctx)
        return;
    fz_drop_context(doc->ctx);
    doc->ctx = NULL;
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
    if (!list->items[0].message) {
        free(list->items);
        free(list);
        return NULL;
    }
    return list;
}

static int attach_single_diag(pdbg_diagnostic_list **out, pdbg_diagnostic_code code, const char *message)
{
    if (!out)
        return 0;
    pdbg_diagnostic_list *list = make_diag_list(code, message);
    if (!list)
        return 0;
    free_diag_list(*out);
    *out = list;
    return 1;
}

static int append_diag(pdbg_diagnostic_list **out, pdbg_diagnostic_code code, const char *message)
{
    if (!out)
        return 0;
    if (!*out)
        return attach_single_diag(out, code, message);

    char *message_copy = copy_string(message);
    if (!message_copy)
        return 0;

    pdbg_diagnostic *items =
        (pdbg_diagnostic *)realloc((*out)->items, ((*out)->len + 1) * sizeof(pdbg_diagnostic));
    if (!items) {
        free(message_copy);
        return 0;
    }

    (*out)->items = items;
    pdbg_diagnostic *item = &(*out)->items[(*out)->len];
    memset(item, 0, sizeof(*item));
    item->severity = PDBG_DIAG_WARNING;
    item->code = code;
    item->message = message_copy;
    (*out)->len += 1;
    return 1;
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

static void free_buffer(pdbg_buffer *buffer)
{
    if (!buffer)
        return;
    free(buffer->data);
    free_diag_list(buffer->diagnostics);
    free(buffer);
}

static void free_image(pdbg_image *image)
{
    if (!image)
        return;
    free(image->pixels);
    free_diag_list(image->diagnostics);
    free(image);
}

static void free_text_page(pdbg_text_page *text)
{
    if (!text)
        return;
    for (size_t i = 0; i < text->len; i++)
        free(text->spans[i].text);
    free(text->spans);
    free(text);
}

static size_t encode_utf8_codepoint(int codepoint, char out[4])
{
    uint32_t c = (uint32_t)codepoint;
    if (codepoint < 0 || c > 0x10FFFFU || (c >= 0xD800U && c <= 0xDFFFU))
        c = 0xFFFDU;

    if (c <= 0x7FU) {
        out[0] = (char)c;
        return 1;
    }
    if (c <= 0x7FFU) {
        out[0] = (char)(0xC0U | (c >> 6));
        out[1] = (char)(0x80U | (c & 0x3FU));
        return 2;
    }
    if (c <= 0xFFFFU) {
        out[0] = (char)(0xE0U | (c >> 12));
        out[1] = (char)(0x80U | ((c >> 6) & 0x3FU));
        out[2] = (char)(0x80U | (c & 0x3FU));
        return 3;
    }

    out[0] = (char)(0xF0U | (c >> 18));
    out[1] = (char)(0x80U | ((c >> 12) & 0x3FU));
    out[2] = (char)(0x80U | ((c >> 6) & 0x3FU));
    out[3] = (char)(0x80U | (c & 0x3FU));
    return 4;
}

static int append_bytes(char **data, size_t *len, size_t *cap, const char *bytes, size_t byte_len)
{
    if (!data || !len || !cap || !bytes)
        return 0;
    if (byte_len > SIZE_MAX - *len - 1)
        return 0;

    size_t needed = *len + byte_len + 1;
    if (needed > *cap) {
        size_t next_cap = *cap ? *cap : 32;
        while (next_cap < needed) {
            if (next_cap > SIZE_MAX / 2) {
                next_cap = needed;
                break;
            }
            next_cap *= 2;
        }
        char *next = (char *)realloc(*data, next_cap);
        if (!next)
            return 0;
        *data = next;
        *cap = next_cap;
    }

    memcpy(*data + *len, bytes, byte_len);
    *len += byte_len;
    (*data)[*len] = '\0';
    return 1;
}

static fz_rect quad_bounds(fz_quad quad)
{
    fz_rect rect;
    rect.x0 = quad.ul.x;
    rect.y0 = quad.ul.y;
    rect.x1 = quad.ul.x;
    rect.y1 = quad.ul.y;

#define PDBG_EXPAND_POINT(point)            \
    do {                                    \
        if ((point).x < rect.x0)            \
            rect.x0 = (point).x;            \
        if ((point).y < rect.y0)            \
            rect.y0 = (point).y;            \
        if ((point).x > rect.x1)            \
            rect.x1 = (point).x;            \
        if ((point).y > rect.y1)            \
            rect.y1 = (point).y;            \
    } while (0)

    PDBG_EXPAND_POINT(quad.ur);
    PDBG_EXPAND_POINT(quad.ll);
    PDBG_EXPAND_POINT(quad.lr);
#undef PDBG_EXPAND_POINT

    return rect;
}

static void union_rect_into(fz_rect *acc, fz_rect rect)
{
    if (rect.x0 < acc->x0)
        acc->x0 = rect.x0;
    if (rect.y0 < acc->y0)
        acc->y0 = rect.y0;
    if (rect.x1 > acc->x1)
        acc->x1 = rect.x1;
    if (rect.y1 > acc->y1)
        acc->y1 = rect.y1;
}

static int append_text_span(
    pdbg_text_page *page,
    uint32_t page_index,
    char *line_text,
    size_t text_len,
    fz_rect bbox,
    fz_rect page_bounds,
    int include_coordinates)
{
    if (!page || !line_text)
        return 0;

    pdbg_text_span *items = (pdbg_text_span *)realloc(page->spans, (page->len + 1) * sizeof(pdbg_text_span));
    if (!items)
        return 0;

    page->spans = items;
    pdbg_text_span *span = &page->spans[page->len];
    memset(span, 0, sizeof(*span));
    span->text = line_text;
    span->text_len = text_len;
    span->page_index = page_index;
    span->untrusted = 1;

    if (include_coordinates) {
        span->x = bbox.x0 - page_bounds.x0;
        span->y = bbox.y0 - page_bounds.y0;
        span->width = bbox.x1 > bbox.x0 ? bbox.x1 - bbox.x0 : 0.0f;
        span->height = bbox.y1 > bbox.y0 ? bbox.y1 - bbox.y0 : 0.0f;
    }

    page->len += 1;
    return 1;
}

static void cancel_cookie_scope_init(pdbg_cancel_cookie_scope *scope)
{
    if (scope)
        memset(scope, 0, sizeof(*scope));
}

static fz_cookie *prepare_cancel_cookie(pdbg_cancel_cookie_scope *scope, pdbg_cancel_token *cancel)
{
    if (!scope || !cancel)
        return NULL;
    cancel_cookie_scope_init(scope);
    scope->token = cancel;
    if (atomic_load(&cancel->cancelled))
        scope->cookie.abort = 1;

    if (!cancel->mutex_initialized)
        return &scope->cookie;

    pthread_mutex_lock(&cancel->mutex);
    if (atomic_load(&cancel->cancelled))
        scope->cookie.abort = 1;
    scope->link.cookie = &scope->cookie;
    scope->link.next = cancel->active_cookies;
    cancel->active_cookies = &scope->link;
    scope->registered = 1;
    pthread_mutex_unlock(&cancel->mutex);

    return &scope->cookie;
}

static void finish_cancel_cookie(pdbg_cancel_cookie_scope *scope)
{
    if (!scope || !scope->registered || !scope->token || !scope->token->mutex_initialized)
        return;

    pdbg_cancel_token *token = scope->token;
    pthread_mutex_lock(&token->mutex);
    struct pdbg_active_cookie **cursor = &token->active_cookies;
    while (*cursor) {
        if (*cursor == &scope->link) {
            *cursor = scope->link.next;
            break;
        }
        cursor = &(*cursor)->next;
    }
    pthread_mutex_unlock(&token->mutex);

    scope->registered = 0;
    scope->token = NULL;
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

static pdbg_object_id object_id_from_ref(fz_context *ctx, pdf_obj *obj);

static int add_stream_filter(pdbg_stream_summary *stream, const char *name)
{
    char **filters = (char **)realloc(stream->filters, (stream->filter_count + 1) * sizeof(char *));
    if (!filters)
        return 0;
    stream->filters = filters;
    stream->filters[stream->filter_count] = copy_string(name ? name : "");
    if (!stream->filters[stream->filter_count])
        return 0;
    stream->filter_count += 1;
    return 1;
}

static int fill_stream_filters(fz_context *ctx, pdbg_stream_summary *stream, pdf_obj *filter)
{
    if (!filter)
        return 1;
    if (pdf_is_name(ctx, filter))
        return add_stream_filter(stream, pdf_to_name(ctx, filter));
    if (pdf_is_array(ctx, filter)) {
        int len = pdf_array_len(ctx, filter);
        for (int i = 0; i < len; i++) {
            pdf_obj *item = pdf_array_get(ctx, filter, i);
            if (pdf_is_name(ctx, item) && !add_stream_filter(stream, pdf_to_name(ctx, item)))
                return 0;
        }
    }
    return 1;
}

static int stream_raw_size_hint(fz_context *ctx, pdf_obj *obj, uint64_t *out);

static int fill_stream_summary(fz_context *ctx, pdbg_stream_summary *stream, pdbg_node_id *node, pdf_obj *obj)
{
    memset(stream, 0, sizeof(*stream));
    if (node->has_object)
        stream->object = node->object;
    else if (pdf_is_indirect(ctx, obj))
        stream->object = object_id_from_ref(ctx, obj);

    uint64_t raw_size = 0;
    if (stream_raw_size_hint(ctx, obj, &raw_size)) {
        stream->raw_size_hint = raw_size;
        stream->has_raw_size_hint = 1;
    }

    if (!fill_stream_filters(ctx, stream, pdf_dict_get(ctx, obj, PDF_NAME(Filter))))
        return 0;

    stream->can_decode = 1;
    return 1;
}

static int stream_raw_size_hint(fz_context *ctx, pdf_obj *obj, uint64_t *out)
{
    if (!out)
        return 0;
    *out = 0;

    pdf_obj *length = pdf_dict_get(ctx, obj, PDF_NAME(Length));
    if (!length || !pdf_is_number(ctx, length))
        return 0;

    int64_t raw_size = pdf_to_int64(ctx, length);
    if (raw_size < 0)
        return 0;

    *out = (uint64_t)raw_size;
    return 1;
}

static int status_can_be_stream_decode_diagnostic(pdbg_status status)
{
    return status == PDBG_ERROR_FORMAT || status == PDBG_ERROR_GENERIC || status == PDBG_ERROR_UNSUPPORTED;
}

static pdbg_status measure_raw_stream_size(
    pdbg_doc *doc,
    int object_num,
    pdbg_cancel_token *cancel,
    uint64_t *out,
    pdbg_error *err)
{
    if (!out) {
        set_error(err, PDBG_ERROR_GENERIC, "raw stream size output is null");
        return PDBG_ERROR_GENERIC;
    }

    *out = 0;
    fz_context *ctx = doc->ctx;
    fz_stream *raw_stream = NULL;
    pdbg_status status = PDBG_OK;
    fz_var(raw_stream);
    fz_var(status);

    fz_try(ctx)
    {
        raw_stream = pdf_open_raw_stream_number(ctx, doc->pdf_doc, object_num);
        unsigned char scratch[8192];
        uint64_t total = 0;
        while (status == PDBG_OK) {
            if (cancel && atomic_load(&cancel->cancelled)) {
                status = PDBG_ERROR_CANCELLED;
                set_error(err, status, "cancelled");
                break;
            }

            size_t n = fz_read(ctx, raw_stream, scratch, sizeof(scratch));
            if (n == 0)
                break;
            if ((uint64_t)n > UINT64_MAX - total) {
                status = PDBG_ERROR_LIMIT;
                set_error(err, status, "raw stream size overflow");
                break;
            }
            total += (uint64_t)n;
        }
        if (status == PDBG_OK)
            *out = total;
    }
    fz_always(ctx)
    {
        if (raw_stream)
            fz_drop_stream(ctx, raw_stream);
    }
    fz_catch(ctx)
    {
        status = set_mupdf_error(ctx, err);
    }

    return status;
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
    pdbg_document_summary_out *out,
    const char *key,
    const char *label)
{
    if (!out || !out->metadata)
        return;

    char value[512];
    int written = fz_lookup_metadata(ctx, doc, key, value, sizeof(value));
    if (written <= 0 || value[0] == '\0')
        return;

    size_t index = out->metadata_len;
    out->metadata[index].key = copy_string(label);
    out->metadata[index].value = copy_string(value);
    if (out->metadata[index].key && out->metadata[index].value)
        out->metadata_len += 1;
    else {
        free(out->metadata[index].key);
        free(out->metadata[index].value);
        memset(&out->metadata[index], 0, sizeof(out->metadata[index]));
    }
}

static void fill_metadata(fz_context *ctx, fz_document *doc, pdbg_document_summary_out *out)
{
    const size_t capacity = 5;
    pdbg_string_pair *pairs = (pdbg_string_pair *)calloc(capacity, sizeof(pdbg_string_pair));
    if (!pairs)
        return;
    out->metadata = pairs;
    out->metadata_len = 0;

    add_metadata_pair(ctx, doc, out, FZ_META_INFO_TITLE, "Title");
    add_metadata_pair(ctx, doc, out, FZ_META_INFO_AUTHOR, "Author");
    add_metadata_pair(ctx, doc, out, FZ_META_INFO_CREATOR, "Creator");
    add_metadata_pair(ctx, doc, out, FZ_META_INFO_PRODUCER, "Producer");
    add_metadata_pair(ctx, doc, out, FZ_META_ENCRYPTION, "Encryption");

    if (out->metadata_len == 0) {
        free(out->metadata);
        out->metadata = NULL;
        return;
    }
}

static int document_is_encrypted(fz_context *ctx, fz_document *doc)
{
    char encryption[128];
    int written = fz_lookup_metadata(ctx, doc, FZ_META_ENCRYPTION, encryption, sizeof(encryption));
    return written > 0 && strcmp(encryption, "None") != 0;
}

static int pdf_name_is(fz_context *ctx, pdf_obj *obj, const char *name)
{
    if (!obj || !name || !pdf_is_name(ctx, obj))
        return 0;
    const char *actual = pdf_to_name(ctx, obj);
    return actual && strcmp(actual, name) == 0;
}

static int action_name_is_external(fz_context *ctx, pdf_obj *obj)
{
    return pdf_name_is(ctx, obj, "URI") || pdf_name_is(ctx, obj, "Launch") ||
           pdf_name_is(ctx, obj, "GoToR") || pdf_name_is(ctx, obj, "SubmitForm") ||
           pdf_name_is(ctx, obj, "ImportData");
}

static int file_spec_type_name(fz_context *ctx, pdf_obj *obj)
{
    return pdf_name_is(ctx, obj, "Filespec") || pdf_name_is(ctx, obj, "FileSpec");
}

static void scan_object_safety_refs(
    fz_context *ctx,
    pdf_obj *obj,
    uint32_t depth,
    uint32_t max_depth,
    size_t *budget,
    int *embedded,
    int *external)
{
    if (!obj || !budget || *budget == 0 || depth > max_depth || (*embedded && *external))
        return;
    if (pdf_is_indirect(ctx, obj))
        return;

    *budget -= 1;

    if (pdf_is_dict(ctx, obj)) {
        int len = pdf_dict_len(ctx, obj);
        int is_file_spec = 0;
        int has_embedded_file_dict = 0;
        int has_file_name = 0;

        for (int i = 0; i < len && (!*embedded || !*external); i++) {
            pdf_obj *key = pdf_dict_get_key(ctx, obj, i);
            pdf_obj *val = pdf_dict_get_val(ctx, obj, i);
            const char *key_name = pdf_is_name(ctx, key) ? pdf_to_name(ctx, key) : "";

            if (strcmp(key_name, "Type") == 0) {
                if (pdf_name_is(ctx, val, "EmbeddedFile"))
                    *embedded = 1;
                if (file_spec_type_name(ctx, val))
                    is_file_spec = 1;
            } else if (strcmp(key_name, "EF") == 0) {
                has_embedded_file_dict = 1;
                *embedded = 1;
            } else if (
                strcmp(key_name, "F") == 0 || strcmp(key_name, "UF") == 0 ||
                strcmp(key_name, "DOS") == 0 || strcmp(key_name, "Mac") == 0 ||
                strcmp(key_name, "Unix") == 0) {
                if (pdf_is_string(ctx, val) || pdf_is_name(ctx, val) || pdf_is_dict(ctx, val))
                    has_file_name = 1;
            } else if (strcmp(key_name, "S") == 0) {
                if (action_name_is_external(ctx, val))
                    *external = 1;
            } else if (strcmp(key_name, "URI") == 0) {
                if (pdf_is_string(ctx, val) || pdf_is_name(ctx, val))
                    *external = 1;
            } else if (strcmp(key_name, "FS") == 0) {
                if (pdf_name_is(ctx, val, "URL"))
                    *external = 1;
            }

            if (val && !pdf_is_indirect(ctx, val))
                scan_object_safety_refs(ctx, val, depth + 1, max_depth, budget, embedded, external);
        }

        if (is_file_spec && has_file_name && !has_embedded_file_dict)
            *external = 1;
    } else if (pdf_is_array(ctx, obj)) {
        int len = pdf_array_len(ctx, obj);
        for (int i = 0; i < len && (!*embedded || !*external); i++) {
            pdf_obj *item = pdf_array_get(ctx, obj, i);
            if (item && !pdf_is_indirect(ctx, item))
                scan_object_safety_refs(ctx, item, depth + 1, max_depth, budget, embedded, external);
        }
    }
}

static void scan_document_safety_refs(fz_context *ctx, pdbg_doc *doc, int *embedded, int *external)
{
    if (!doc || !doc->pdf_doc || !embedded || !external)
        return;

    int xref_len = pdf_xref_len(ctx, doc->pdf_doc);
    size_t budget = 4096;
    uint32_t max_depth = doc->max_object_depth ? doc->max_object_depth : PDBG_DEFAULT_MAX_OBJECT_DEPTH;

    for (int num = 1; num < xref_len && (!*embedded || !*external) && budget > 0; num++) {
        pdf_obj *obj = NULL;
        fz_var(obj);
        fz_try(ctx)
        {
            obj = pdf_load_object(ctx, doc->pdf_doc, num);
            scan_object_safety_refs(ctx, obj, 0, max_depth, &budget, embedded, external);
        }
        fz_always(ctx)
        {
            if (obj)
                pdf_drop_obj(ctx, obj);
        }
        fz_catch(ctx)
        {
            /* Safety scanning is best-effort; damaged objects are reported by other paths. */
        }
    }
}

static pdbg_object_id object_id_from_ref(fz_context *ctx, pdf_obj *obj)
{
    pdbg_object_id id;
    id.num = pdf_to_num(ctx, obj);
    id.gen = pdf_to_gen(ctx, obj);
    return id;
}

static pdbg_node_id direct_node(uint64_t document_id, pdbg_node_kind kind)
{
    pdbg_node_id node;
    memset(&node, 0, sizeof(node));
    node.document_id = document_id;
    node.kind = kind;
    return node;
}

static pdbg_node_id object_node(uint64_t document_id, pdbg_node_kind kind, pdbg_object_id object)
{
    pdbg_node_id node = direct_node(document_id, kind);
    node.object = object;
    node.has_object = 1;
    return node;
}

static pdbg_node_id page_node(uint64_t document_id, uint32_t page_index)
{
    pdbg_node_id node = direct_node(document_id, PDBG_NODE_PAGE);
    node.page_index = page_index;
    return node;
}

static pdf_obj *registry_lookup(pdbg_doc *doc, uint64_t token)
{
    if (!doc || token == 0)
        return NULL;
    for (size_t i = 0; i < doc->path_len; i++) {
        if (doc->paths[i].token == token)
            return doc->paths[i].obj;
    }
    return NULL;
}

static uint64_t registry_keep(fz_context *ctx, pdbg_doc *doc, pdf_obj *obj)
{
    if (!doc || !obj)
        return 0;
    if (doc->path_len == doc->path_cap) {
        size_t next_cap = doc->path_cap ? doc->path_cap * 2 : 64;
        struct pdbg_path_binding *next =
            (struct pdbg_path_binding *)realloc(doc->paths, next_cap * sizeof(struct pdbg_path_binding));
        if (!next)
            return 0;
        doc->paths = next;
        doc->path_cap = next_cap;
    }

    uint64_t token = doc->next_path_token++;
    if (token == 0)
        token = doc->next_path_token++;
    doc->paths[doc->path_len].token = token;
    doc->paths[doc->path_len].obj = pdf_keep_obj(ctx, obj);
    doc->path_len += 1;
    return token;
}

static void registry_drop(fz_context *ctx, pdbg_doc *doc)
{
    if (!doc || !doc->paths)
        return;
    for (size_t i = 0; i < doc->path_len; i++)
        pdf_drop_obj(ctx, doc->paths[i].obj);
    free(doc->paths);
    doc->paths = NULL;
    doc->path_len = 0;
    doc->path_cap = 0;
}

static pdbg_object_kind object_kind_for(fz_context *ctx, pdf_obj *obj)
{
    if (!obj)
        return PDBG_OBJECT_UNKNOWN;
    if (pdf_is_stream(ctx, obj))
        return PDBG_OBJECT_STREAM;
    if (pdf_is_null(ctx, obj))
        return PDBG_OBJECT_NULL;
    if (pdf_is_bool(ctx, obj))
        return PDBG_OBJECT_BOOL;
    if (pdf_is_int(ctx, obj))
        return PDBG_OBJECT_INT;
    if (pdf_is_real(ctx, obj))
        return PDBG_OBJECT_REAL;
    if (pdf_is_name(ctx, obj))
        return PDBG_OBJECT_NAME;
    if (pdf_is_string(ctx, obj))
        return PDBG_OBJECT_STRING;
    if (pdf_is_array(ctx, obj))
        return PDBG_OBJECT_ARRAY;
    if (pdf_is_dict(ctx, obj))
        return PDBG_OBJECT_DICT;
    if (pdf_is_indirect(ctx, obj))
        return PDBG_OBJECT_INDIRECT_REF;
    return PDBG_OBJECT_UNKNOWN;
}

static size_t object_child_count(fz_context *ctx, pdf_obj *obj)
{
    if (!obj)
        return 0;
    if (pdf_is_dict(ctx, obj))
        return (size_t)pdf_dict_len(ctx, obj);
    if (pdf_is_array(ctx, obj))
        return (size_t)pdf_array_len(ctx, obj);
    return 0;
}

static int object_exceeds_depth(fz_context *ctx, pdf_obj *obj, uint32_t max_depth, uint32_t depth)
{
    if (!obj || max_depth == 0)
        return 0;
    if (depth > max_depth)
        return 1;

    if (pdf_is_dict(ctx, obj)) {
        int len = pdf_dict_len(ctx, obj);
        for (int i = 0; i < len; i++) {
            if (object_exceeds_depth(ctx, pdf_dict_get_val(ctx, obj, i), max_depth, depth + 1))
                return 1;
        }
    } else if (pdf_is_array(ctx, obj)) {
        int len = pdf_array_len(ctx, obj);
        for (int i = 0; i < len; i++) {
            if (object_exceeds_depth(ctx, pdf_array_get(ctx, obj, i), max_depth, depth + 1))
                return 1;
        }
    }

    return 0;
}

static char *object_preview(fz_context *ctx, pdf_obj *obj, uint32_t max_depth)
{
    if (!obj)
        return copy_string("");
    if (object_exceeds_depth(ctx, obj, max_depth, 1))
        return copy_string("<object preview exceeds max depth>");

    char preview[256];
    size_t len = 0;
    preview[0] = '\0';
    char *rendered = pdf_sprint_obj(ctx, preview, sizeof(preview), &len, obj, 1, 1);
    if (!rendered)
        return copy_string("");

    char *out = copy_string(rendered);
    if (rendered != preview)
        fz_free(ctx, rendered);
    return out;
}

static void fill_value(fz_context *ctx, pdf_obj *obj, pdbg_object_value *value)
{
    memset(value, 0, sizeof(*value));
    if (!obj) {
        value->kind = PDBG_VALUE_UNKNOWN;
        return;
    }
    if (pdf_is_null(ctx, obj)) {
        value->kind = PDBG_VALUE_NULL;
    } else if (pdf_is_bool(ctx, obj)) {
        value->kind = PDBG_VALUE_BOOL;
        value->bool_value = pdf_to_bool(ctx, obj);
    } else if (pdf_is_int(ctx, obj)) {
        value->kind = PDBG_VALUE_INT;
        value->int_value = pdf_to_int64(ctx, obj);
    } else if (pdf_is_real(ctx, obj)) {
        value->kind = PDBG_VALUE_REAL;
        value->real_value = pdf_to_real(ctx, obj);
    } else if (pdf_is_name(ctx, obj)) {
        value->kind = PDBG_VALUE_NAME;
        value->name_value = copy_string(pdf_to_name(ctx, obj));
    } else if (pdf_is_string(ctx, obj)) {
        size_t len = 0;
        const char *bytes = pdf_to_string(ctx, obj, &len);
        value->kind = PDBG_VALUE_STRING_BYTES;
        value->string_kind = PDBG_STRING_UNKNOWN;
        value->bytes = (uint8_t *)malloc(len);
        if (value->bytes && len)
            memcpy(value->bytes, bytes, len);
        value->byte_len = value->bytes ? len : 0;
        value->is_text_string = 1;
        value->decoded_text = copy_string(pdf_to_text_string(ctx, obj));
    } else if (pdf_is_indirect(ctx, obj)) {
        value->kind = PDBG_VALUE_INDIRECT_REF;
        value->ref_value = object_id_from_ref(ctx, obj);
    } else if (pdf_is_array(ctx, obj) || pdf_is_dict(ctx, obj) || pdf_is_stream(ctx, obj)) {
        value->kind = PDBG_VALUE_CONTAINER;
    } else {
        value->kind = PDBG_VALUE_UNKNOWN;
    }
}

static pdbg_node_list *alloc_node_list(size_t total, size_t offset, size_t limit)
{
    size_t remaining = offset < total ? total - offset : 0;
    size_t len = remaining < limit ? remaining : limit;
    pdbg_node_list *list = (pdbg_node_list *)calloc(1, sizeof(pdbg_node_list));
    if (!list)
        return NULL;
    list->len = len;
    list->has_total = 1;
    list->total = total;
    if (len == 0)
        return list;
    list->items = (pdbg_dict_entry *)calloc(len, sizeof(pdbg_dict_entry));
    if (!list->items) {
        free(list);
        return NULL;
    }
    return list;
}

static int fill_entry_common(
    fz_context *ctx,
    pdbg_dict_entry *entry,
    const char *key,
    pdbg_node_id node,
    pdbg_object_kind kind,
    const char *label,
    const char *preview)
{
    memset(entry, 0, sizeof(*entry));
    entry->key = copy_string(key);
    entry->node = node;
    entry->object_kind = kind;
    entry->label = copy_string(label);
    entry->preview = copy_string(preview);
    (void)ctx;
    return entry->key && entry->label && entry->preview;
}

static int fill_entry_for_obj(
    fz_context *ctx,
    pdbg_doc *doc,
    pdbg_dict_entry *entry,
    const char *key,
    pdf_obj *raw_obj)
{
    pdf_obj *resolved = pdf_resolve_indirect(ctx, raw_obj);
    uint64_t token = registry_keep(ctx, doc, resolved ? resolved : raw_obj);
    if (token == 0)
        return 0;

    pdbg_node_id node = direct_node(doc->document_id, PDBG_NODE_PATH_TOKEN);
    node.path_token = token;
    if (pdf_is_indirect(ctx, raw_obj)) {
        node.object = object_id_from_ref(ctx, raw_obj);
        node.has_object = 1;
    }

    char label[128];
    if (key && key[0] != '\0')
        snprintf(label, sizeof(label), "%s", key);
    else
        snprintf(label, sizeof(label), "item");

    char *preview = object_preview(ctx, raw_obj, doc->max_object_depth);
    if (!preview)
        return 0;

    int ok = fill_entry_common(ctx, entry, key ? key : "", node, object_kind_for(ctx, resolved), label, preview);
    free(preview);
    if (!ok)
        return 0;

    if (pdf_is_indirect(ctx, raw_obj)) {
        entry->object = object_id_from_ref(ctx, raw_obj);
        entry->has_object = 1;
    }
    entry->has_stream = pdf_is_stream(ctx, raw_obj) ||
        (node.has_object && pdf_obj_num_is_stream(ctx, doc->pdf_doc, node.object.num));
    entry->child_count = object_child_count(ctx, resolved);
    entry->has_child_count = entry->child_count > 0;
    entry->has_children = entry->has_child_count || entry->has_stream;
    return 1;
}

static pdf_obj *catalog_obj(fz_context *ctx, pdf_document *pdf)
{
    pdf_obj *root = pdf_dict_get(ctx, pdf_trailer(ctx, pdf), PDF_NAME(Root));
    return pdf_resolve_indirect(ctx, root);
}

static pdf_obj *page_root_obj(fz_context *ctx, pdf_document *pdf)
{
    pdf_obj *catalog = catalog_obj(ctx, pdf);
    pdf_obj *pages = catalog ? pdf_dict_get(ctx, catalog, PDF_NAME(Pages)) : NULL;
    return pdf_resolve_indirect(ctx, pages);
}

static pdf_obj *resolve_node_obj(fz_context *ctx, pdbg_doc *doc, const pdbg_node_id *node, int *drop_after)
{
    *drop_after = 0;
    if (!node)
        return NULL;

    switch (node->kind) {
    case PDBG_NODE_TRAILER:
        return pdf_trailer(ctx, doc->pdf_doc);
    case PDBG_NODE_CATALOG:
        return catalog_obj(ctx, doc->pdf_doc);
    case PDBG_NODE_PAGE_ROOT:
        return page_root_obj(ctx, doc->pdf_doc);
    case PDBG_NODE_PAGE:
        return pdf_lookup_page_obj(ctx, doc->pdf_doc, (int)node->page_index);
    case PDBG_NODE_PATH_TOKEN:
        return registry_lookup(doc, node->path_token);
    case PDBG_NODE_XREF_OBJECT:
    case PDBG_NODE_INDIRECT_REF:
    case PDBG_NODE_STREAM:
        if (!node->has_object)
            return NULL;
        *drop_after = 1;
        return pdf_load_object(ctx, doc->pdf_doc, node->object.num);
    default:
        return NULL;
    }
}

static pdbg_node_list *document_root_children(fz_context *ctx, pdbg_doc *doc, size_t offset, size_t limit)
{
    const size_t total = 4;
    pdbg_node_list *list = NULL;
    int ok = 1;
    fz_var(list);
    fz_var(ok);

    fz_try(ctx)
    {
        list = alloc_node_list(total, offset, limit);
        if (!list)
            ok = 0;

        for (size_t i = 0; ok && i < list->len; i++) {
            size_t child = offset + i;
            pdbg_dict_entry *entry = &list->items[i];
            int entry_ok = 0;
            if (child == 0) {
                pdf_obj *trailer = pdf_trailer(ctx, doc->pdf_doc);
                entry_ok = fill_entry_common(
                    ctx,
                    entry,
                    "Trailer",
                    direct_node(doc->document_id, PDBG_NODE_TRAILER),
                    PDBG_OBJECT_TRAILER,
                    "Trailer",
                    "PDF trailer dictionary");
                entry->has_children = 1;
                entry->child_count = object_child_count(ctx, trailer);
                entry->has_child_count = 1;
            } else if (child == 1) {
                pdf_obj *root_ref = pdf_dict_get(ctx, pdf_trailer(ctx, doc->pdf_doc), PDF_NAME(Root));
                pdf_obj *catalog = pdf_resolve_indirect(ctx, root_ref);
                pdbg_node_id node = direct_node(doc->document_id, PDBG_NODE_CATALOG);
                if (pdf_is_indirect(ctx, root_ref)) {
                    node.object = object_id_from_ref(ctx, root_ref);
                    node.has_object = 1;
                }
                entry_ok = fill_entry_common(ctx, entry, "Catalog", node, PDBG_OBJECT_DICT, "Catalog", "Document catalog");
                if (node.has_object) {
                    entry->object = node.object;
                    entry->has_object = 1;
                }
                entry->has_children = 1;
                entry->child_count = object_child_count(ctx, catalog);
                entry->has_child_count = 1;
            } else if (child == 2) {
                int page_count = fz_count_pages(ctx, doc->fz_doc);
                entry_ok = fill_entry_common(
                    ctx,
                    entry,
                    "Pages",
                    direct_node(doc->document_id, PDBG_NODE_PAGE_ROOT),
                    PDBG_OBJECT_ARRAY,
                    "Pages",
                    "Page tree");
                entry->has_children = page_count > 0;
                entry->child_count = (size_t)page_count;
                entry->has_child_count = 1;
            } else if (child == 3) {
                int xref_len = pdf_xref_len(ctx, doc->pdf_doc);
                entry_ok = fill_entry_common(
                    ctx,
                    entry,
                    "Xref",
                    direct_node(doc->document_id, PDBG_NODE_XREF_ROOT),
                    PDBG_OBJECT_XREF_ENTRY,
                    "Xref",
                    "Cross-reference table");
                entry->has_children = xref_len > 1;
                entry->child_count = xref_len > 0 ? (size_t)(xref_len - 1) : 0;
                entry->has_child_count = 1;
            }
            ok = entry_ok;
        }
    }
    fz_catch(ctx)
    {
        pdbg_node_list_drop(list);
        fz_rethrow(ctx);
    }

    if (!ok) {
        pdbg_node_list_drop(list);
        return NULL;
    }
    return list;
}

static pdbg_node_list *page_root_children(fz_context *ctx, pdbg_doc *doc, size_t offset, size_t limit)
{
    pdbg_node_list *list = NULL;
    int ok = 1;
    fz_var(list);
    fz_var(ok);

    fz_try(ctx)
    {
        int page_count = fz_count_pages(ctx, doc->fz_doc);
        size_t total = page_count > 0 ? (size_t)page_count : 0;
        list = alloc_node_list(total, offset, limit);
        if (!list)
            ok = 0;

        for (size_t i = 0; ok && i < list->len; i++) {
            size_t page_index = offset + i;
            char key[32];
            char label[64];
            snprintf(key, sizeof(key), "%zu", page_index);
            snprintf(label, sizeof(label), "Page %zu", page_index + 1);
            ok = fill_entry_common(
                ctx,
                &list->items[i],
                key,
                page_node(doc->document_id, (uint32_t)page_index),
                PDBG_OBJECT_PAGE,
                label,
                "Page object");
            list->items[i].has_children = 1;
        }
    }
    fz_catch(ctx)
    {
        pdbg_node_list_drop(list);
        fz_rethrow(ctx);
    }

    if (!ok) {
        pdbg_node_list_drop(list);
        return NULL;
    }
    return list;
}

static pdbg_node_list *xref_root_children(fz_context *ctx, pdbg_doc *doc, size_t offset, size_t limit)
{
    pdbg_node_list *list = NULL;
    int ok = 1;
    fz_var(list);
    fz_var(ok);

    fz_try(ctx)
    {
        int xref_len = pdf_xref_len(ctx, doc->pdf_doc);
        size_t total = xref_len > 1 ? (size_t)(xref_len - 1) : 0;
        list = alloc_node_list(total, offset, limit);
        if (!list)
            ok = 0;

        for (size_t i = 0; ok && i < list->len; i++) {
            int object_num = (int)(offset + i + 1);
            char key[32];
            char label[64];
            char preview[64];
            snprintf(key, sizeof(key), "%d", object_num);
            snprintf(label, sizeof(label), "Object %d 0 R", object_num);
            snprintf(preview, sizeof(preview), "%d 0 R", object_num);
            pdbg_object_id object;
            object.num = object_num;
            object.gen = 0;
            ok = fill_entry_common(
                ctx,
                &list->items[i],
                key,
                object_node(doc->document_id, PDBG_NODE_XREF_OBJECT, object),
                PDBG_OBJECT_XREF_ENTRY,
                label,
                preview);
            list->items[i].object = object;
            list->items[i].has_object = 1;
            list->items[i].has_children = 1;
        }
    }
    fz_catch(ctx)
    {
        pdbg_node_list_drop(list);
        fz_rethrow(ctx);
    }

    if (!ok) {
        pdbg_node_list_drop(list);
        return NULL;
    }
    return list;
}

static pdbg_node_list *object_children(
    fz_context *ctx,
    pdbg_doc *doc,
    pdf_obj *obj,
    size_t offset,
    size_t limit)
{
    if (!obj)
        return alloc_node_list(0, offset, limit);

    pdbg_node_list *list = NULL;
    int ok = 1;
    fz_var(list);
    fz_var(ok);

    fz_try(ctx)
    {
        if (pdf_is_dict(ctx, obj)) {
            size_t total = (size_t)pdf_dict_len(ctx, obj);
            list = alloc_node_list(total, offset, limit);
            if (!list)
                ok = 0;
            for (size_t i = 0; ok && i < list->len; i++) {
                int dict_index = (int)(offset + i);
                const char *key = pdf_to_name(ctx, pdf_dict_get_key(ctx, obj, dict_index));
                pdf_obj *val = pdf_dict_get_val(ctx, obj, dict_index);
                ok = fill_entry_for_obj(ctx, doc, &list->items[i], key, val);
            }
        } else if (pdf_is_array(ctx, obj)) {
            size_t total = (size_t)pdf_array_len(ctx, obj);
            list = alloc_node_list(total, offset, limit);
            if (!list)
                ok = 0;
            for (size_t i = 0; ok && i < list->len; i++) {
                size_t item_index = offset + i;
                char key[32];
                snprintf(key, sizeof(key), "%zu", item_index);
                pdf_obj *val = pdf_array_get(ctx, obj, (int)item_index);
                ok = fill_entry_for_obj(ctx, doc, &list->items[i], key, val);
            }
        } else {
            list = alloc_node_list(0, offset, limit);
            if (!list)
                ok = 0;
        }
    }
    fz_catch(ctx)
    {
        pdbg_node_list_drop(list);
        fz_rethrow(ctx);
    }

    if (!ok) {
        pdbg_node_list_drop(list);
        return NULL;
    }
    return list;
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

    if (pthread_mutex_init(&ctx->open_mutex, NULL) != 0) {
        free(ctx);
        set_error(err, PDBG_ERROR_GENERIC, "failed to initialize MuPDF open lock");
        return PDBG_ERROR_GENERIC;
    }
    ctx->open_mutex_initialized = 1;

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
    fz_set_error_callback(ctx->ctx, ignore_mupdf_message, NULL);
    fz_set_warning_callback(ctx->ctx, ignore_mupdf_message, NULL);

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
    if (pthread_mutex_init(&token->mutex, NULL) != 0) {
        free(token);
        set_error(err, PDBG_ERROR_GENERIC, "failed to initialize cancel token mutex");
        return PDBG_ERROR_GENERIC;
    }
    token->mutex_initialized = 1;

    *out = token;
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

void pdbg_cancel_token_cancel(pdbg_cancel_token *token)
{
    if (token) {
        atomic_store(&token->cancelled, 1);
        if (token->mutex_initialized) {
            pthread_mutex_lock(&token->mutex);
            for (struct pdbg_active_cookie *active = token->active_cookies; active; active = active->next) {
                if (active->cookie) {
                    /* Intentional MuPDF async-abort write; lifetime is mutex-protected above. */
                    active->cookie->abort = 1;
                }
            }
            pthread_mutex_unlock(&token->mutex);
        }
    }
}

void pdbg_cancel_token_drop(pdbg_cancel_token *token)
{
    if (token && token->mutex_initialized)
        pthread_mutex_destroy(&token->mutex);
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
    if (!clone_doc_context(ctx, doc, err)) {
        free(doc);
        return PDBG_ERROR_OOM;
    }
    doc->document_id = (uint64_t)atomic_fetch_add_explicit(&next_document_id, 1, memory_order_relaxed);
    doc->next_path_token = 1;
    doc->file_path = copy_string(path);
    apply_open_options(doc, options);
    if (!doc->file_path) {
        drop_doc_context(doc);
        free(doc);
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    fz_context *doc_ctx = doc->ctx;
    fz_document *opened = NULL;
    pdf_document *pdf = NULL;
    pdbg_status status = PDBG_OK;
    fz_var(opened);
    fz_var(pdf);
    fz_var(status);

    fz_try(doc_ctx)
    {
        opened = fz_open_document(doc_ctx, path);
        pdf = pdf_specifics(doc_ctx, opened);
        if (!pdf) {
            status = PDBG_ERROR_UNSUPPORTED;
            set_error(err, status, "opened document is not a PDF");
        } else {
            pdf_disable_js(doc_ctx, pdf);
            doc->javascript_disabled = 1;

            doc->encrypted = document_is_encrypted(doc_ctx, opened);
            doc->needs_password = fz_needs_password(doc_ctx, opened);
            doc->authenticated = doc->needs_password ? 0 : 1;

            if (doc->needs_password && password && password[0] != '\0') {
                if (fz_authenticate_password(doc_ctx, opened, password)) {
                    doc->needs_password = 0;
                    doc->authenticated = 1;
                } else {
                    status = PDBG_ERROR_PASSWORD;
                    set_error(err, status, "password authentication failed");
                }
            }

            if (status == PDBG_OK && doc->authenticated) {
                doc->repaired_or_damaged = pdf_was_repaired(doc_ctx, pdf);
                if (repair_is_forbidden(doc)) {
                    status = PDBG_ERROR_FORMAT;
                    set_error(err, status, "document required repair but repair policy forbids it");
                }
            }
        }
    }
    fz_catch(doc_ctx)
    {
        status = set_mupdf_error(doc_ctx, err);
    }

    if (status != PDBG_OK) {
        if (opened)
            fz_drop_document(doc_ctx, opened);
        free(doc->file_path);
        drop_doc_context(doc);
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
    if (!ctx || fd < 0 || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid open_fd arguments");
        return PDBG_ERROR_GENERIC;
    }
    *out = NULL;

    int dup_fd = dup(fd);
    if (dup_fd < 0) {
        set_error(err, PDBG_ERROR_GENERIC, strerror(errno));
        return PDBG_ERROR_GENERIC;
    }

    FILE *file = fdopen(dup_fd, "rb");
    if (!file) {
        int saved_errno = errno;
        close(dup_fd);
        set_error(err, PDBG_ERROR_GENERIC, strerror(saved_errno));
        return PDBG_ERROR_GENERIC;
    }

    pdbg_doc *doc = (pdbg_doc *)calloc(1, sizeof(pdbg_doc));
    if (!doc) {
        fclose(file);
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    const char *path = display_path && display_path[0] != '\0' ? display_path : "<fd>";
    doc->owner = ctx;
    if (!clone_doc_context(ctx, doc, err)) {
        fclose(file);
        free(doc);
        return PDBG_ERROR_OOM;
    }
    doc->owned_file = file;
    doc->document_id = (uint64_t)atomic_fetch_add_explicit(&next_document_id, 1, memory_order_relaxed);
    doc->next_path_token = 1;
    doc->file_path = copy_string(path);
    apply_open_options(doc, options);
    if (!doc->file_path) {
        drop_doc_context(doc);
        fclose(file);
        free(doc);
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    fz_context *doc_ctx = doc->ctx;
    fz_stream *stream = NULL;
    fz_document *opened = NULL;
    pdf_document *pdf = NULL;
    pdbg_status status = PDBG_OK;
    fz_var(stream);
    fz_var(opened);
    fz_var(pdf);
    fz_var(status);

    fz_try(doc_ctx)
    {
        stream = fz_open_file_ptr_no_close(doc_ctx, file);
        opened = fz_open_document_with_stream(doc_ctx, "application/pdf", stream);
        pdf = pdf_specifics(doc_ctx, opened);
        if (!pdf) {
            status = PDBG_ERROR_UNSUPPORTED;
            set_error(err, status, "opened fd is not a PDF");
        } else {
            pdf_disable_js(doc_ctx, pdf);
            doc->javascript_disabled = 1;

            doc->encrypted = document_is_encrypted(doc_ctx, opened);
            doc->needs_password = fz_needs_password(doc_ctx, opened);
            doc->authenticated = doc->needs_password ? 0 : 1;

            if (doc->needs_password && password && password[0] != '\0') {
                if (fz_authenticate_password(doc_ctx, opened, password)) {
                    doc->needs_password = 0;
                    doc->authenticated = 1;
                } else {
                    status = PDBG_ERROR_PASSWORD;
                    set_error(err, status, "password authentication failed");
                }
            }

            if (status == PDBG_OK && doc->authenticated) {
                doc->repaired_or_damaged = pdf_was_repaired(doc_ctx, pdf);
                if (repair_is_forbidden(doc)) {
                    status = PDBG_ERROR_FORMAT;
                    set_error(err, status, "document required repair but repair policy forbids it");
                }
            }
        }
    }
    fz_always(doc_ctx)
    {
        if (stream)
            fz_drop_stream(doc_ctx, stream);
    }
    fz_catch(doc_ctx)
    {
        status = set_mupdf_error(doc_ctx, err);
    }

    if (status != PDBG_OK) {
        if (opened)
            fz_drop_document(doc_ctx, opened);
        free(doc->file_path);
        drop_doc_context(doc);
        fclose(file);
        free(doc);
        return status;
    }

    doc->fz_doc = opened;
    doc->pdf_doc = pdf;
    *out = doc;
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

void pdbg_document_drop(pdbg_doc *doc)
{
    if (!doc)
        return;
    if (doc->ctx)
        registry_drop(doc->ctx, doc);
    if (doc->ctx && doc->fz_doc)
        fz_drop_document(doc->ctx, doc->fz_doc);
    if (doc->owned_file)
        fclose(doc->owned_file);
    free(doc->file_path);
    drop_doc_context(doc);
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

    fz_context *ctx = doc->ctx;
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

            if (status == PDBG_OK) {
                int embedded = 0;
                int external = 0;
                scan_document_safety_refs(ctx, doc, &embedded, &external);
                out->embedded_files_detected = embedded;
                out->external_references_detected = external;
                if (embedded &&
                    !append_diag(
                        &out->diagnostics,
                        PDBG_DIAG_EMBEDDED_FILE_DETECTED,
                        "embedded files detected; automatic extraction is disabled")) {
                    status = PDBG_ERROR_OOM;
                    set_error(err, status, "out of memory");
                }
                if (status == PDBG_OK && external &&
                    !append_diag(
                        &out->diagnostics,
                        PDBG_DIAG_EXTERNAL_REFERENCE_DETECTED,
                        doc->allow_external_references
                            ? "external references detected; automatic following is not performed by this shim"
                            : "external references detected; safe mode will not follow them")) {
                    status = PDBG_ERROR_OOM;
                    set_error(err, status, "out of memory");
                }
            }

            if (status == PDBG_OK && pdf_was_repaired(ctx, doc->pdf_doc)) {
                out->repaired_or_damaged = 1;
                if (!append_diag(&out->diagnostics, PDBG_DIAG_REPAIR_WARNING, "MuPDF repaired the document on open")) {
                    status = PDBG_ERROR_OOM;
                    set_error(err, status, "out of memory");
                }
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
    if (!doc || !node || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid children arguments");
        return PDBG_ERROR_GENERIC;
    }
    *out = NULL;
    if (!doc->authenticated) {
        set_error(err, PDBG_ERROR_PASSWORD, "document requires password before tree traversal");
        return PDBG_ERROR_PASSWORD;
    }

    fz_context *ctx = doc->ctx;
    pdbg_status status = PDBG_OK;
    pdbg_node_list *list = NULL;
    int drop_after = 0;
    pdf_obj *obj = NULL;
    fz_var(status);
    fz_var(list);
    fz_var(obj);
    fz_var(drop_after);

    fz_try(ctx)
    {
        switch (node->kind) {
        case PDBG_NODE_DOCUMENT_ROOT:
            list = document_root_children(ctx, doc, offset, limit);
            break;
        case PDBG_NODE_PAGE_ROOT:
            list = page_root_children(ctx, doc, offset, limit);
            break;
        case PDBG_NODE_XREF_ROOT:
            list = xref_root_children(ctx, doc, offset, limit);
            break;
        default:
            obj = resolve_node_obj(ctx, doc, node, &drop_after);
            list = object_children(ctx, doc, obj, offset, limit);
            break;
        }
        if (!list) {
            status = PDBG_ERROR_OOM;
            set_error(err, status, "out of memory");
        }
    }
    fz_catch(ctx)
    {
        status = set_mupdf_error(ctx, err);
    }

    if (drop_after && obj)
        pdf_drop_obj(ctx, obj);
    if (status != PDBG_OK) {
        pdbg_node_list_drop(list);
        return status;
    }

    *out = list;
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

pdbg_status pdbg_object_detail(
    pdbg_doc *doc,
    const pdbg_node_id *node,
    pdbg_object_detail_out *out,
    pdbg_error *err)
{
    if (!doc || !node || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid detail arguments");
        return PDBG_ERROR_GENERIC;
    }
    if (!doc->authenticated) {
        memset(out, 0, sizeof(*out));
        set_error(err, PDBG_ERROR_PASSWORD, "document requires password before object detail");
        return PDBG_ERROR_PASSWORD;
    }

    memset(out, 0, sizeof(*out));
    out->id = *node;

    fz_context *ctx = doc->ctx;
    pdbg_status status = PDBG_OK;
    int drop_after = 0;
    pdf_obj *obj = NULL;
    fz_var(status);
    fz_var(drop_after);
    fz_var(obj);

    fz_try(ctx)
    {
        obj = resolve_node_obj(ctx, doc, node, &drop_after);
        if (node->kind == PDBG_NODE_DOCUMENT_ROOT) {
            out->kind = PDBG_OBJECT_DICT;
            out->label = copy_string("Document");
            out->preview = copy_string("PDF document");
            out->value.kind = PDBG_VALUE_CONTAINER;
            out->dictionary_entries = document_root_children(ctx, doc, 0, 64);
            if (!out->dictionary_entries) {
                status = PDBG_ERROR_OOM;
                set_error(err, status, "out of memory");
            }
        } else if (node->kind == PDBG_NODE_XREF_ROOT) {
            out->kind = PDBG_OBJECT_XREF_ENTRY;
            out->label = copy_string("Xref");
            out->preview = copy_string("Cross-reference table");
            out->value.kind = PDBG_VALUE_CONTAINER;
            out->children = xref_root_children(ctx, doc, 0, 64);
            if (!out->children) {
                status = PDBG_ERROR_OOM;
                set_error(err, status, "out of memory");
            }
        } else if (node->kind == PDBG_NODE_PAGE_ROOT) {
            out->kind = PDBG_OBJECT_ARRAY;
            out->label = copy_string("Pages");
            out->preview = copy_string("Page tree");
            out->value.kind = PDBG_VALUE_CONTAINER;
            out->children = page_root_children(ctx, doc, 0, 64);
            if (!out->children) {
                status = PDBG_ERROR_OOM;
                set_error(err, status, "out of memory");
            }
        } else if (obj) {
            out->kind = node->kind == PDBG_NODE_TRAILER ? PDBG_OBJECT_TRAILER : object_kind_for(ctx, obj);
            out->label = copy_string("Object");
            out->preview = object_preview(ctx, obj, doc->max_object_depth);
            fill_value(ctx, obj, &out->value);
            if (node->has_object) {
                out->object = node->object;
                out->has_object = 1;
            }
            if (pdf_is_dict(ctx, obj)) {
                out->dictionary_entries = object_children(ctx, doc, obj, 0, 64);
                if (!out->dictionary_entries) {
                    status = PDBG_ERROR_OOM;
                    set_error(err, status, "out of memory");
                }
            } else if (pdf_is_array(ctx, obj)) {
                out->children = object_children(ctx, doc, obj, 0, 64);
                if (!out->children) {
                    status = PDBG_ERROR_OOM;
                    set_error(err, status, "out of memory");
                }
            }
            int has_stream = pdf_is_stream(ctx, obj) ||
                (node->has_object && pdf_obj_num_is_stream(ctx, doc->pdf_doc, node->object.num));
            if (has_stream) {
                out->has_stream = 1;
                if (!fill_stream_summary(ctx, &out->stream, &out->id, obj)) {
                    status = PDBG_ERROR_OOM;
                    set_error(err, status, "out of memory");
                }
            }
        } else {
            out->kind = PDBG_OBJECT_UNKNOWN;
            out->label = copy_string("Unknown");
            out->preview = copy_string("");
            out->value.kind = PDBG_VALUE_UNKNOWN;
        }

        if (!out->label || !out->preview) {
            status = PDBG_ERROR_OOM;
            set_error(err, status, "out of memory");
        }
    }
    fz_catch(ctx)
    {
        status = set_mupdf_error(ctx, err);
    }

    if (drop_after && obj)
        pdf_drop_obj(ctx, obj);
    if (status != PDBG_OK) {
        pdbg_object_detail_out_drop(out);
        return status;
    }

    set_error(err, PDBG_OK, "");
    return PDBG_OK;
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
    if (!doc || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid stream arguments");
        return PDBG_ERROR_GENERIC;
    }
    *out = NULL;
    if (!doc->authenticated) {
        set_error(err, PDBG_ERROR_PASSWORD, "document requires password before stream load");
        return PDBG_ERROR_PASSWORD;
    }
    if (cancel && atomic_load(&cancel->cancelled)) {
        set_error(err, PDBG_ERROR_CANCELLED, "cancelled");
        return PDBG_ERROR_CANCELLED;
    }
    if (object.num <= 0) {
        set_error(err, PDBG_ERROR_UNSUPPORTED, "object is not a stream");
        return PDBG_ERROR_UNSUPPORTED;
    }
    if ((uint64_t)limit > doc->max_store_bytes) {
        set_error(err, PDBG_ERROR_LIMIT, "requested stream chunk exceeds configured output limit");
        return PDBG_ERROR_LIMIT;
    }

    fz_context *ctx = doc->ctx;
    fz_stream *stream = NULL;
    pdbg_buffer *buffer = NULL;
    uint64_t total = 0;
    size_t copied = 0;
    uint64_t request_end = UINT64_MAX;
    unsigned char scratch[8192];
    pdbg_status status = PDBG_OK;
    fz_var(stream);
    fz_var(buffer);
    fz_var(total);
    fz_var(copied);
    fz_var(status);
    if ((uint64_t)limit <= UINT64_MAX - offset)
        request_end = offset + (uint64_t)limit;

    fz_try(ctx)
    {
        if (!pdf_obj_num_is_stream(ctx, doc->pdf_doc, object.num)) {
            status = PDBG_ERROR_UNSUPPORTED;
            set_error(err, status, "object is not a stream");
        } else {
            buffer = (pdbg_buffer *)calloc(1, sizeof(pdbg_buffer));
            if (!buffer) {
                status = PDBG_ERROR_OOM;
                set_error(err, status, "out of memory");
            } else if (limit > 0) {
                buffer->data = (uint8_t *)malloc(limit);
                if (!buffer->data) {
                    status = PDBG_ERROR_OOM;
                    set_error(err, status, "out of memory");
                }
            }

            if (status == PDBG_OK) {
                uint64_t measured_raw_size = 0;
                if (decoded && doc->max_filter_expansion_ratio > 0) {
                    status = measure_raw_stream_size(doc, object.num, cancel, &measured_raw_size, err);
                }

                if (status == PDBG_OK)
                    stream = decoded ? pdf_open_stream_number(ctx, doc->pdf_doc, object.num)
                                     : pdf_open_raw_stream_number(ctx, doc->pdf_doc, object.num);

                while (status == PDBG_OK) {
                    if (cancel && atomic_load(&cancel->cancelled)) {
                        status = PDBG_ERROR_CANCELLED;
                        set_error(err, status, "cancelled");
                        break;
                    }

                    size_t n = fz_read(ctx, stream, scratch, sizeof(scratch));
                    if (n == 0)
                        break;
                    if ((uint64_t)n > UINT64_MAX - total) {
                        status = PDBG_ERROR_LIMIT;
                        set_error(err, status, "stream size overflow");
                        break;
                    }

                    uint64_t chunk_start = total;
                    uint64_t chunk_end = total + (uint64_t)n;
                    if (decoded && chunk_end > doc->max_decoded_stream_bytes) {
                        status = PDBG_ERROR_LIMIT;
                        set_error(err, status, "decoded stream limit exceeded during decode");
                        break;
                    }
                    if (decoded && doc->max_filter_expansion_ratio > 0) {
                        if (measured_raw_size == 0 && chunk_end > 0) {
                            status = PDBG_ERROR_LIMIT;
                            set_error(err, status, "decoded stream expansion ratio exceeded");
                            break;
                        }
                        if (measured_raw_size > 0 &&
                            (measured_raw_size > UINT64_MAX / (uint64_t)doc->max_filter_expansion_ratio ||
                             chunk_end > measured_raw_size * (uint64_t)doc->max_filter_expansion_ratio)) {
                            status = PDBG_ERROR_LIMIT;
                            set_error(err, status, "decoded stream expansion ratio exceeded");
                            break;
                        }
                    }

                    if (limit > 0 && copied < limit && chunk_end > offset && chunk_start < request_end) {
                        uint64_t copy_start = offset > chunk_start ? offset : chunk_start;
                        uint64_t copy_end = chunk_end < request_end ? chunk_end : request_end;
                        if (copy_end > copy_start) {
                            size_t scratch_offset = (size_t)(copy_start - chunk_start);
                            size_t copy_len = (size_t)(copy_end - copy_start);
                            memcpy(buffer->data + copied, scratch + scratch_offset, copy_len);
                            copied += copy_len;
                        }
                    }

                    total = chunk_end;
                }

                if (status == PDBG_OK) {
                    buffer->len = copied;
                    buffer->total_size = total;
                    uint64_t visible_end = UINT64_MAX;
                    if ((uint64_t)copied <= UINT64_MAX - offset)
                        visible_end = offset + (uint64_t)copied;
                    buffer->truncated = offset < total && visible_end < total;
                }
            }
        }
    }
    fz_always(ctx)
    {
        if (stream)
            fz_drop_stream(ctx, stream);
    }
    fz_catch(ctx)
    {
        pdbg_status caught_status = set_mupdf_error(ctx, err);
        if (decoded && buffer && status_can_be_stream_decode_diagnostic(caught_status)) {
            buffer->len = copied;
            buffer->total_size = total;
            buffer->truncated = 1;
            if (!attach_single_diag(
                    &buffer->diagnostics,
                    PDBG_DIAG_STREAM_DECODE_FAILURE,
                    fz_caught_message(ctx))) {
                status = PDBG_ERROR_OOM;
                set_error(err, status, "out of memory");
            } else {
                buffer->diagnostics->items[0].object = object;
                buffer->diagnostics->items[0].has_object = 1;
                status = PDBG_OK;
            }
        } else {
            status = caught_status;
        }
    }

    if (status != PDBG_OK) {
        free_buffer(buffer);
        return status;
    }

    *out = buffer;
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

pdbg_status pdbg_page_render(
    pdbg_doc *doc,
    uint32_t page_index,
    const pdbg_render_options *options,
    pdbg_cancel_token *cancel,
    pdbg_image **out,
    pdbg_error *err)
{
    if (!doc || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid render arguments");
        return PDBG_ERROR_GENERIC;
    }
    *out = NULL;
    if (!doc->authenticated) {
        set_error(err, PDBG_ERROR_PASSWORD, "document requires password before page render");
        return PDBG_ERROR_PASSWORD;
    }
    if (cancel && atomic_load(&cancel->cancelled)) {
        set_error(err, PDBG_ERROR_CANCELLED, "cancelled");
        return PDBG_ERROR_CANCELLED;
    }

    float zoom = options && options->zoom > 0.0f ? options->zoom : 1.0f;
    int rotation = options ? options->rotation_degrees : 0;
    uint32_t max_width = options && options->max_width ? options->max_width : 4096;
    uint32_t max_height = options && options->max_height ? options->max_height : 4096;
    uint64_t max_pixels = options && options->max_pixels ? options->max_pixels : 16777216ULL;
    uint64_t max_output_bytes = options && options->max_output_bytes ? options->max_output_bytes : 128ULL * 1024ULL * 1024ULL;
    pdbg_color_mode color_mode = options ? options->color_mode : PDBG_COLOR_RGBA;

    if (!(rotation == 0 || rotation == 90 || rotation == 180 || rotation == 270)) {
        set_error(err, PDBG_ERROR_LIMIT, "render rotation must be 0, 90, 180, or 270 degrees");
        return PDBG_ERROR_LIMIT;
    }

    fz_context *ctx = doc->ctx;
    fz_page *page = NULL;
    fz_pixmap *pixmap = NULL;
    fz_device *device = NULL;
    pdbg_image *image = NULL;
    pdbg_cancel_cookie_scope cancel_scope;
    pdbg_status status = PDBG_OK;
    fz_var(page);
    fz_var(pixmap);
    fz_var(device);
    fz_var(image);
    cancel_cookie_scope_init(&cancel_scope);
    fz_var(cancel_scope);
    fz_var(status);

    fz_try(ctx)
    {
        int page_count = fz_count_pages(ctx, doc->fz_doc);
        if (page_index >= (uint32_t)page_count) {
            status = PDBG_ERROR_LIMIT;
            set_error(err, status, "page index out of range");
        } else {
            page = fz_load_page(ctx, doc->fz_doc, (int)page_index);
            fz_matrix transform = fz_pre_rotate(fz_scale(zoom, zoom), (float)rotation);
            fz_rect page_bounds = fz_bound_page(ctx, page);
            fz_rect transformed = fz_transform_rect(page_bounds, transform);
            fz_irect bbox = fz_round_rect(transformed);
            int width = bbox.x1 - bbox.x0;
            int height = bbox.y1 - bbox.y0;

            if (width <= 0 || height <= 0) {
                status = PDBG_ERROR_LIMIT;
                set_error(err, status, "render output has empty dimensions");
            } else if ((uint32_t)width > max_width || (uint32_t)height > max_height) {
                status = PDBG_ERROR_LIMIT;
                set_error(err, status, "render output exceeds configured dimensions");
            } else if ((uint64_t)width > UINT64_MAX / (uint64_t)height ||
                       (uint64_t)width * (uint64_t)height > max_pixels) {
                status = PDBG_ERROR_LIMIT;
                set_error(err, status, "render output exceeds configured pixel count");
            } else {
                pixmap = fz_new_pixmap_with_bbox(ctx, fz_device_rgb(ctx), bbox, NULL, 1);
                fz_clear_pixmap_with_value(ctx, pixmap, 0xFF);
                device = fz_new_draw_device(ctx, transform, pixmap);
                fz_cookie *cookie = prepare_cancel_cookie(&cancel_scope, cancel);
                fz_run_page(ctx, page, device, fz_identity, cookie);
                fz_close_device(ctx, device);

                if ((cancel && atomic_load(&cancel->cancelled)) || (cookie && cookie->abort)) {
                    status = PDBG_ERROR_CANCELLED;
                    set_error(err, status, "cancelled");
                }

                if (status == PDBG_OK) {
                    int stride = fz_pixmap_stride(ctx, pixmap);
                    if (stride <= 0 ||
                        (uint64_t)stride < (uint64_t)width * 4ULL ||
                        (uint64_t)stride > UINT64_MAX / (uint64_t)height) {
                        status = PDBG_ERROR_LIMIT;
                        set_error(err, status, "render output byte size overflow");
                    } else {
                        uint64_t byte_len = (uint64_t)stride * (uint64_t)height;
                        if (byte_len > max_output_bytes || byte_len > SIZE_MAX) {
                            status = PDBG_ERROR_LIMIT;
                            set_error(err, status, "render output exceeds configured byte limit");
                        } else {
                            image = (pdbg_image *)calloc(1, sizeof(pdbg_image));
                            if (!image) {
                                status = PDBG_ERROR_OOM;
                                set_error(err, status, "out of memory");
                            } else {
                                image->pixels = (uint8_t *)malloc((size_t)byte_len);
                                if (!image->pixels) {
                                    status = PDBG_ERROR_OOM;
                                    set_error(err, status, "out of memory");
                                } else {
                                    image->width = (uint32_t)width;
                                    image->height = (uint32_t)height;
                                    image->stride = (size_t)stride;
                                    memcpy(image->pixels, fz_pixmap_samples(ctx, pixmap), (size_t)byte_len);

                                    if (color_mode == PDBG_COLOR_GRAYSCALE || color_mode == PDBG_COLOR_INVERTED) {
                                        for (uint32_t y = 0; y < image->height; y++) {
                                            uint8_t *row = image->pixels + (size_t)y * image->stride;
                                            for (uint32_t x = 0; x < image->width; x++) {
                                                uint8_t *px = row + (size_t)x * 4;
                                                if (color_mode == PDBG_COLOR_GRAYSCALE) {
                                                    uint8_t gray = (uint8_t)((30U * px[0] + 59U * px[1] + 11U * px[2]) / 100U);
                                                    px[0] = gray;
                                                    px[1] = gray;
                                                    px[2] = gray;
                                                } else {
                                                    px[0] = (uint8_t)(255U - px[0]);
                                                    px[1] = (uint8_t)(255U - px[1]);
                                                    px[2] = (uint8_t)(255U - px[2]);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    fz_always(ctx)
    {
        finish_cancel_cookie(&cancel_scope);
        if (device)
            fz_drop_device(ctx, device);
        if (pixmap)
            fz_drop_pixmap(ctx, pixmap);
        if (page)
            fz_drop_page(ctx, page);
    }
    fz_catch(ctx)
    {
        status = set_mupdf_error(ctx, err);
    }

    if (status != PDBG_OK) {
        free_image(image);
        return status;
    }

    *out = image;
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

pdbg_status pdbg_page_extract_text(
    pdbg_doc *doc,
    uint32_t page_index,
    const pdbg_text_options *options,
    pdbg_cancel_token *cancel,
    pdbg_text_page **out,
    pdbg_error *err)
{
    if (!doc || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid text arguments");
        return PDBG_ERROR_GENERIC;
    }
    *out = NULL;
    if (!doc->authenticated) {
        set_error(err, PDBG_ERROR_PASSWORD, "document requires password before text extraction");
        return PDBG_ERROR_PASSWORD;
    }
    if (cancel && atomic_load(&cancel->cancelled)) {
        set_error(err, PDBG_ERROR_CANCELLED, "cancelled");
        return PDBG_ERROR_CANCELLED;
    }

    size_t max_chars = options && options->max_chars ? options->max_chars : (size_t)PDBG_DEFAULT_MAX_TEXT_CHARS;
    size_t max_blocks = options && options->max_blocks ? options->max_blocks : (size_t)PDBG_DEFAULT_MAX_TEXT_BLOCKS;
    int include_coordinates = !options || options->include_coordinates;

    fz_context *ctx = doc->ctx;
    fz_page *page = NULL;
    fz_stext_page *stext = NULL;
    fz_device *device = NULL;
    pdbg_text_page *text = NULL;
    pdbg_cancel_cookie_scope cancel_scope;
    pdbg_status status = PDBG_OK;
    fz_var(page);
    fz_var(stext);
    fz_var(device);
    fz_var(text);
    cancel_cookie_scope_init(&cancel_scope);
    fz_var(cancel_scope);
    fz_var(status);

    fz_try(ctx)
    {
        int page_count = fz_count_pages(ctx, doc->fz_doc);
        if (page_index >= (uint32_t)page_count) {
            status = PDBG_ERROR_LIMIT;
            set_error(err, status, "page index out of range");
        } else {
            page = fz_load_page(ctx, doc->fz_doc, (int)page_index);
            fz_rect page_bounds = fz_bound_page(ctx, page);
            fz_stext_options stext_options;
            memset(&stext_options, 0, sizeof(stext_options));
            stext_options.flags = FZ_STEXT_PRESERVE_WHITESPACE | FZ_STEXT_MEDIABOX_CLIP;
            stext_options.scale = 1.0f;
            if (!options || options->sort_by_position)
                stext_options.flags |= FZ_STEXT_SEGMENT;

            stext = fz_new_stext_page(ctx, page_bounds);
            device = fz_new_stext_device(ctx, stext, &stext_options);
            fz_cookie *cookie = prepare_cancel_cookie(&cancel_scope, cancel);
            fz_run_page(ctx, page, device, fz_identity, cookie);
            fz_close_device(ctx, device);

            if ((cancel && atomic_load(&cancel->cancelled)) || (cookie && cookie->abort)) {
                status = PDBG_ERROR_CANCELLED;
                set_error(err, status, "cancelled");
            }

            if (status == PDBG_OK) {
                text = (pdbg_text_page *)calloc(1, sizeof(pdbg_text_page));
                if (!text) {
                    status = PDBG_ERROR_OOM;
                    set_error(err, status, "out of memory");
                }
            }

            size_t block_count = 0;
            size_t total_chars = 0;
            for (fz_stext_block *block = status == PDBG_OK ? stext->first_block : NULL;
                 block && status == PDBG_OK;
                 block = block->next) {
                if (block->type != FZ_STEXT_BLOCK_TEXT)
                    continue;

                block_count += 1;
                if (block_count > max_blocks) {
                    status = PDBG_ERROR_LIMIT;
                    set_error(err, status, "text extraction exceeded configured block limit");
                    break;
                }

                for (fz_stext_line *line = block->u.t.first_line; line && status == PDBG_OK; line = line->next) {
                    char *line_text = NULL;
                    size_t line_len = 0;
                    size_t line_cap = 0;
                    fz_rect line_bbox;
                    int has_bbox = 0;

                    for (fz_stext_char *ch = line->first_char; ch && status == PDBG_OK; ch = ch->next) {
                        if (cancel && atomic_load(&cancel->cancelled)) {
                            status = PDBG_ERROR_CANCELLED;
                            set_error(err, status, "cancelled");
                            break;
                        }
                        if (total_chars >= max_chars) {
                            status = PDBG_ERROR_LIMIT;
                            set_error(err, status, "text extraction exceeded configured character limit");
                            break;
                        }

                        int codepoint = ch->c;
                        if (ch->flags & (FZ_STEXT_UNICODE_IS_CID | FZ_STEXT_UNICODE_IS_GID))
                            codepoint = 0xFFFD;
                        char encoded[4];
                        size_t encoded_len = encode_utf8_codepoint(codepoint, encoded);
                        if (!append_bytes(&line_text, &line_len, &line_cap, encoded, encoded_len)) {
                            status = PDBG_ERROR_OOM;
                            set_error(err, status, "out of memory");
                            break;
                        }
                        total_chars += 1;

                        fz_rect char_bbox = quad_bounds(ch->quad);
                        if (!has_bbox) {
                            line_bbox = char_bbox;
                            has_bbox = 1;
                        } else {
                            union_rect_into(&line_bbox, char_bbox);
                        }
                    }

                    if (status == PDBG_OK && line_len > 0) {
                        if (!append_text_span(
                                text,
                                page_index,
                                line_text,
                                line_len,
                                has_bbox ? line_bbox : page_bounds,
                                page_bounds,
                                include_coordinates)) {
                            status = PDBG_ERROR_OOM;
                            set_error(err, status, "out of memory");
                        } else {
                            line_text = NULL;
                        }
                    }
                    free(line_text);
                }
            }
        }
    }
    fz_always(ctx)
    {
        finish_cancel_cookie(&cancel_scope);
        if (device)
            fz_drop_device(ctx, device);
        if (stext)
            fz_drop_stext_page(ctx, stext);
        if (page)
            fz_drop_page(ctx, page);
    }
    fz_catch(ctx)
    {
        status = set_mupdf_error(ctx, err);
    }

    if (status != PDBG_OK) {
        free_text_page(text);
        return status;
    }

    *out = text;
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

void pdbg_buffer_drop(pdbg_buffer *buffer)
{
    free_buffer(buffer);
}

void pdbg_image_drop(pdbg_image *image)
{
    free_image(image);
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
    free_text_page(text);
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
    if (!doc || !doc->owned_file)
        return -1;
    return fileno(doc->owned_file);
}

int pdbg_test_fd_is_open(int fd)
{
    if (fd < 0)
        return 0;
    return fcntl(fd, F_GETFD) == -1 ? 0 : 1;
}
