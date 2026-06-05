#include "pdbg_shim.h"

#include <mupdf/fitz.h>
#include <mupdf/pdf.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct pdbg_context {
    int placeholder;
};

struct pdbg_doc {
    int placeholder;
};

struct pdbg_buffer {
    int placeholder;
};

struct pdbg_image {
    int placeholder;
};

struct pdbg_node_list {
    int placeholder;
};

struct pdbg_diagnostic_list {
    int placeholder;
};

struct pdbg_text_page {
    int placeholder;
};

struct pdbg_cancel_token {
    int cancelled;
};

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

static pdbg_status unsupported(pdbg_error *err, const char *operation)
{
    char message[256];
    snprintf(message, sizeof(message), "%s is not implemented in the M1.1 real-mupdf skeleton", operation);
    set_error(err, PDBG_ERROR_UNSUPPORTED, message);
    return PDBG_ERROR_UNSUPPORTED;
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
    (void)ctx;
    (void)path;
    (void)password;
    (void)options;
    if (out)
        *out = NULL;
    return unsupported(err, "pdbg_document_open");
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
    free(doc);
}

pdbg_status pdbg_document_summary(pdbg_doc *doc, pdbg_document_summary_out *out, pdbg_error *err)
{
    (void)doc;
    if (out)
        memset(out, 0, sizeof(*out));
    return unsupported(err, "pdbg_document_summary");
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
    free(buffer);
}

void pdbg_image_drop(pdbg_image *image)
{
    free(image);
}

void pdbg_node_list_drop(pdbg_node_list *list)
{
    free(list);
}

void pdbg_text_page_drop(pdbg_text_page *text)
{
    free(text);
}

void pdbg_document_summary_out_drop(pdbg_document_summary_out *out)
{
    if (out)
        memset(out, 0, sizeof(*out));
}

void pdbg_object_detail_out_drop(pdbg_object_detail_out *out)
{
    if (out)
        memset(out, 0, sizeof(*out));
}

const uint8_t *pdbg_buffer_data(const pdbg_buffer *buffer)
{
    (void)buffer;
    return NULL;
}

size_t pdbg_buffer_len(const pdbg_buffer *buffer)
{
    (void)buffer;
    return 0;
}

uint64_t pdbg_buffer_total_size_hint(const pdbg_buffer *buffer)
{
    (void)buffer;
    return 0;
}

int pdbg_buffer_truncated(const pdbg_buffer *buffer)
{
    (void)buffer;
    return 0;
}

size_t pdbg_buffer_diagnostic_count(const pdbg_buffer *buffer)
{
    (void)buffer;
    return 0;
}

pdbg_status pdbg_buffer_diagnostic_get(
    const pdbg_buffer *buffer,
    size_t index,
    pdbg_diagnostic *out,
    pdbg_error *err)
{
    (void)buffer;
    (void)index;
    (void)out;
    return unsupported(err, "pdbg_buffer_diagnostic_get");
}

uint32_t pdbg_image_width(const pdbg_image *image)
{
    (void)image;
    return 0;
}

uint32_t pdbg_image_height(const pdbg_image *image)
{
    (void)image;
    return 0;
}

size_t pdbg_image_stride(const pdbg_image *image)
{
    (void)image;
    return 0;
}

const uint8_t *pdbg_image_rgba_pixels(const pdbg_image *image)
{
    (void)image;
    return NULL;
}

size_t pdbg_image_diagnostic_count(const pdbg_image *image)
{
    (void)image;
    return 0;
}

pdbg_status pdbg_image_diagnostic_get(
    const pdbg_image *image,
    size_t index,
    pdbg_diagnostic *out,
    pdbg_error *err)
{
    (void)image;
    (void)index;
    (void)out;
    return unsupported(err, "pdbg_image_diagnostic_get");
}

size_t pdbg_node_list_len(const pdbg_node_list *list)
{
    (void)list;
    return 0;
}

int pdbg_node_list_has_total_count(const pdbg_node_list *list)
{
    (void)list;
    return 0;
}

size_t pdbg_node_list_total_count(const pdbg_node_list *list)
{
    (void)list;
    return 0;
}

pdbg_status pdbg_node_list_get(
    const pdbg_node_list *list,
    size_t index,
    pdbg_dict_entry *out,
    pdbg_error *err)
{
    (void)list;
    (void)index;
    (void)out;
    return unsupported(err, "pdbg_node_list_get");
}

size_t pdbg_diagnostic_list_len(const pdbg_diagnostic_list *list)
{
    (void)list;
    return 0;
}

pdbg_status pdbg_diagnostic_list_get(
    const pdbg_diagnostic_list *list,
    size_t index,
    pdbg_diagnostic *out,
    pdbg_error *err)
{
    (void)list;
    (void)index;
    (void)out;
    return unsupported(err, "pdbg_diagnostic_list_get");
}

size_t pdbg_text_page_span_count(const pdbg_text_page *text)
{
    (void)text;
    return 0;
}

pdbg_status pdbg_text_page_span_get(
    const pdbg_text_page *text,
    size_t index,
    pdbg_text_span *out,
    pdbg_error *err)
{
    (void)text;
    (void)index;
    (void)out;
    return unsupported(err, "pdbg_text_page_span_get");
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
