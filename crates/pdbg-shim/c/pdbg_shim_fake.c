#include "pdbg_shim.h"

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

struct pdbg_context {
    uint64_t id;
};

struct pdbg_doc {
    uint64_t document_id;
    int owns_fd;
    int fd_dup;
    uint64_t max_decoded_stream_bytes;
};

struct pdbg_cancel_token {
    int cancelled;
};

struct pdbg_diagnostic_list {
    size_t len;
    pdbg_diagnostic *items;
};

struct pdbg_node_list {
    size_t len;
    int has_total;
    size_t total;
    pdbg_dict_entry *items;
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

struct pdbg_text_page {
    size_t len;
    pdbg_text_span *spans;
};

static uint64_t next_context_id = 1;
static uint64_t next_document_id = 1;

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

static pdbg_node_id fake_node(uint64_t document_id, pdbg_node_kind kind, uint64_t token)
{
    pdbg_node_id node;
    memset(&node, 0, sizeof(node));
    node.document_id = document_id;
    node.kind = kind;
    node.path_token = token;
    node.object.num = 1;
    node.object.gen = 0;
    node.has_object = 1;
    return node;
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
    list->items[0].has_object = 1;
    list->items[0].object.num = 1;
    list->items[0].object.gen = 0;
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

static pdbg_stream_summary make_stream_summary(void)
{
    pdbg_stream_summary stream;
    memset(&stream, 0, sizeof(stream));
    stream.object.num = 1;
    stream.object.gen = 0;
    stream.filter_count = 1;
    stream.filters = (char **)calloc(1, sizeof(char *));
    if (stream.filters)
        stream.filters[0] = copy_string("FlateDecode");
    stream.raw_size_hint = 32;
    stream.has_raw_size_hint = 1;
    stream.decoded_size_hint = 64;
    stream.has_decoded_size_hint = 1;
    stream.can_decode = 1;
    stream.image_preview_available = 0;
    return stream;
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

static pdbg_node_list *make_node_list(uint64_t document_id, size_t offset, size_t limit)
{
    const size_t total = 3;
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

    for (size_t i = 0; i < len; i++) {
        size_t child_index = offset + i;
        char key_buf[32];
        char label_buf[32];
        snprintf(key_buf, sizeof(key_buf), "Key%zu", child_index);
        snprintf(label_buf, sizeof(label_buf), "Object %zu", child_index);

        pdbg_dict_entry *entry = &list->items[i];
        entry->key = copy_string(key_buf);
        entry->node = fake_node(document_id, PDBG_NODE_PATH_TOKEN, child_index + 1);
        entry->object_kind = PDBG_OBJECT_DICT;
        entry->object.num = (int)(child_index + 1);
        entry->object.gen = 0;
        entry->has_object = 1;
        entry->label = copy_string(label_buf);
        entry->preview = copy_string("fake object");
        entry->has_children = 1;
        entry->has_stream = child_index == 0 ? 1 : 0;
        entry->child_count = 3;
        entry->has_child_count = 1;
        entry->byte_size_hint = 128;
        entry->has_byte_size_hint = 1;
        entry->max_diagnostic_severity = PDBG_DIAG_WARNING;
        entry->diagnostic_count = 1;
        entry->diagnostics = make_diag_list(PDBG_DIAG_REPAIR_WARNING, "fake child diagnostic");
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

    ctx->id = next_context_id++;
    *out = ctx;
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

void pdbg_context_drop(pdbg_context *ctx)
{
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
    (void)path;
    (void)password;

    if (!ctx || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid open arguments");
        return PDBG_ERROR_GENERIC;
    }
    if (path && strcmp(path, "fail-open") == 0) {
        set_error(err, PDBG_ERROR_GENERIC, "fake open failure");
        return PDBG_ERROR_GENERIC;
    }

    pdbg_doc *doc = (pdbg_doc *)calloc(1, sizeof(pdbg_doc));
    if (!doc) {
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    doc->document_id = next_document_id++;
    doc->fd_dup = -1;
    doc->max_decoded_stream_bytes =
        options && options->max_decoded_stream_bytes ? options->max_decoded_stream_bytes : 1024 * 1024;
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
    (void)display_path;
    (void)password;

    if (!ctx || !out || fd < 0) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid open_fd arguments");
        return PDBG_ERROR_GENERIC;
    }

    int dup_fd = dup(fd);
    if (dup_fd < 0) {
        set_error(err, PDBG_ERROR_GENERIC, strerror(errno));
        return PDBG_ERROR_GENERIC;
    }

    pdbg_status status = pdbg_document_open(ctx, display_path, password, options, out, err);
    if (status != PDBG_OK) {
        close(dup_fd);
        return status;
    }

    (*out)->owns_fd = 1;
    (*out)->fd_dup = dup_fd;
    return PDBG_OK;
}

void pdbg_document_drop(pdbg_doc *doc)
{
    if (!doc)
        return;
    if (doc->owns_fd && doc->fd_dup >= 0)
        close(doc->fd_dup);
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
    out->file_path = copy_string("fake.pdf");
    out->file_hash = copy_string("fake-hash");
    out->pdf_version = copy_string("1.7");
    out->page_count = 1;
    out->xref_size = 3;
    out->parsed_object_count = 3;
    out->has_parsed_object_count = 1;
    out->permissions.print = 1;
    out->permissions.copy = 1;
    out->safe_mode = 1;
    out->javascript_disabled = 1;
    out->diagnostics = make_diag_list(PDBG_DIAG_REPAIR_WARNING, "fake document diagnostic");
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
    (void)node;
    if (!doc || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid children arguments");
        return PDBG_ERROR_GENERIC;
    }

    *out = make_node_list(doc->document_id, offset, limit);
    if (!*out) {
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    set_error(err, PDBG_OK, "");
    return PDBG_OK;
}

pdbg_status pdbg_object_detail(
    pdbg_doc *doc,
    const pdbg_node_id *node,
    pdbg_object_detail_out *out,
    pdbg_error *err)
{
    if (!doc || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid detail arguments");
        return PDBG_ERROR_GENERIC;
    }

    memset(out, 0, sizeof(*out));
    out->id = node ? *node : fake_node(doc->document_id, PDBG_NODE_DOCUMENT_ROOT, 0);
    out->object.num = 1;
    out->object.gen = 0;
    out->has_object = 1;
    out->kind = PDBG_OBJECT_DICT;
    out->label = copy_string("Fake object");
    out->preview = copy_string("<< /Type /Fake >>");
    out->value.kind = PDBG_VALUE_CONTAINER;
    out->children = make_node_list(doc->document_id, 0, 2);
    out->dictionary_entries = make_node_list(doc->document_id, 0, 2);
    out->has_stream = 1;
    out->stream = make_stream_summary();
    out->diagnostics = make_diag_list(PDBG_DIAG_REPAIR_WARNING, "fake object diagnostic");
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
    (void)object;
    (void)offset;

    if (!doc || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid stream arguments");
        return PDBG_ERROR_GENERIC;
    }
    if (cancel && cancel->cancelled) {
        set_error(err, PDBG_ERROR_CANCELLED, "cancelled");
        return PDBG_ERROR_CANCELLED;
    }
    if (decoded && limit > doc->max_decoded_stream_bytes) {
        set_error(err, PDBG_ERROR_LIMIT, "decoded stream limit exceeded during decode");
        return PDBG_ERROR_LIMIT;
    }

    static const uint8_t bytes[] = "fake stream bytes";
    size_t data_len = sizeof(bytes) - 1;
    if (limit < data_len)
        data_len = limit;

    pdbg_buffer *buffer = (pdbg_buffer *)calloc(1, sizeof(pdbg_buffer));
    if (!buffer) {
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    if (data_len) {
        buffer->data = (uint8_t *)malloc(data_len);
        if (!buffer->data) {
            free(buffer);
            set_error(err, PDBG_ERROR_OOM, "out of memory");
            return PDBG_ERROR_OOM;
        }
        memcpy(buffer->data, bytes, data_len);
    }
    buffer->len = data_len;
    buffer->total_size = sizeof(bytes) - 1;
    buffer->truncated = data_len < sizeof(bytes) - 1;
    buffer->diagnostics = make_diag_list(PDBG_DIAG_STREAM_DECODE_FAILURE, "fake stream diagnostic");
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
    (void)page_index;
    (void)options;
    if (!doc || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid render arguments");
        return PDBG_ERROR_GENERIC;
    }
    if (cancel && cancel->cancelled) {
        set_error(err, PDBG_ERROR_CANCELLED, "cancelled");
        return PDBG_ERROR_CANCELLED;
    }

    pdbg_image *image = (pdbg_image *)calloc(1, sizeof(pdbg_image));
    if (!image) {
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    image->width = 1;
    image->height = 1;
    image->stride = 4;
    image->pixels = (uint8_t *)malloc(4);
    if (!image->pixels) {
        free(image);
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }
    image->pixels[0] = 255;
    image->pixels[1] = 255;
    image->pixels[2] = 255;
    image->pixels[3] = 255;
    image->diagnostics = make_diag_list(PDBG_DIAG_RENDER_WARNING, "fake render diagnostic");
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
    (void)options;
    if (!doc || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid text arguments");
        return PDBG_ERROR_GENERIC;
    }
    if (cancel && cancel->cancelled) {
        set_error(err, PDBG_ERROR_CANCELLED, "cancelled");
        return PDBG_ERROR_CANCELLED;
    }

    pdbg_text_page *page = (pdbg_text_page *)calloc(1, sizeof(pdbg_text_page));
    if (!page) {
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    page->len = 1;
    page->spans = (pdbg_text_span *)calloc(1, sizeof(pdbg_text_span));
    if (!page->spans) {
        free(page);
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }

    char *text = (char *)malloc(4);
    if (!text) {
        free(page->spans);
        free(page);
        set_error(err, PDBG_ERROR_OOM, "out of memory");
        return PDBG_ERROR_OOM;
    }
    text[0] = 'A';
    text[1] = '\0';
    text[2] = 'B';
    text[3] = '\0';
    page->spans[0].text = text;
    page->spans[0].text_len = 3;
    page->spans[0].width = 10.0f;
    page->spans[0].height = 12.0f;
    page->spans[0].page_index = page_index;
    page->spans[0].untrusted = 1;
    *out = page;
    set_error(err, PDBG_OK, "");
    return PDBG_OK;
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
