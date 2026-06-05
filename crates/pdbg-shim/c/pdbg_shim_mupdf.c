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
    int cancelled;
};

static atomic_uint_fast64_t next_document_id = 1;

struct pdbg_path_binding {
    uint64_t token;
    pdf_obj *obj;
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

static char *object_preview(fz_context *ctx, pdf_obj *obj)
{
    if (!obj)
        return copy_string("");
    char preview[256];
    size_t len = 0;
    preview[0] = '\0';
    pdf_sprint_obj(ctx, preview, sizeof(preview), &len, obj, 1, 1);
    preview[sizeof(preview) - 1] = '\0';
    return copy_string(preview);
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

    char *preview = object_preview(ctx, raw_obj);
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
    entry->has_stream = pdf_is_stream(ctx, resolved);
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
    pdbg_node_list *list = alloc_node_list(total, offset, limit);
    if (!list)
        return NULL;

    for (size_t i = 0; i < list->len; i++) {
        size_t child = offset + i;
        pdbg_dict_entry *entry = &list->items[i];
        int ok = 0;
        if (child == 0) {
            pdf_obj *trailer = pdf_trailer(ctx, doc->pdf_doc);
            ok = fill_entry_common(
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
            ok = fill_entry_common(ctx, entry, "Catalog", node, PDBG_OBJECT_DICT, "Catalog", "Document catalog");
            if (node.has_object) {
                entry->object = node.object;
                entry->has_object = 1;
            }
            entry->has_children = 1;
            entry->child_count = object_child_count(ctx, catalog);
            entry->has_child_count = 1;
        } else if (child == 2) {
            int page_count = fz_count_pages(ctx, doc->fz_doc);
            ok = fill_entry_common(
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
            ok = fill_entry_common(
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
        if (!ok) {
            pdbg_node_list_drop(list);
            return NULL;
        }
    }
    return list;
}

static pdbg_node_list *page_root_children(fz_context *ctx, pdbg_doc *doc, size_t offset, size_t limit)
{
    int page_count = fz_count_pages(ctx, doc->fz_doc);
    size_t total = page_count > 0 ? (size_t)page_count : 0;
    pdbg_node_list *list = alloc_node_list(total, offset, limit);
    if (!list)
        return NULL;

    for (size_t i = 0; i < list->len; i++) {
        size_t page_index = offset + i;
        char key[32];
        char label[64];
        snprintf(key, sizeof(key), "%zu", page_index);
        snprintf(label, sizeof(label), "Page %zu", page_index + 1);
        if (!fill_entry_common(
                ctx,
                &list->items[i],
                key,
                page_node(doc->document_id, (uint32_t)page_index),
                PDBG_OBJECT_PAGE,
                label,
                "Page object")) {
            pdbg_node_list_drop(list);
            return NULL;
        }
        list->items[i].has_children = 1;
    }
    return list;
}

static pdbg_node_list *xref_root_children(fz_context *ctx, pdbg_doc *doc, size_t offset, size_t limit)
{
    int xref_len = pdf_xref_len(ctx, doc->pdf_doc);
    size_t total = xref_len > 1 ? (size_t)(xref_len - 1) : 0;
    pdbg_node_list *list = alloc_node_list(total, offset, limit);
    if (!list)
        return NULL;

    for (size_t i = 0; i < list->len; i++) {
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
        if (!fill_entry_common(
                ctx,
                &list->items[i],
                key,
                object_node(doc->document_id, PDBG_NODE_XREF_OBJECT, object),
                PDBG_OBJECT_XREF_ENTRY,
                label,
                preview)) {
            pdbg_node_list_drop(list);
            return NULL;
        }
        list->items[i].object = object;
        list->items[i].has_object = 1;
        list->items[i].has_children = 1;
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

    if (pdf_is_dict(ctx, obj)) {
        size_t total = (size_t)pdf_dict_len(ctx, obj);
        pdbg_node_list *list = alloc_node_list(total, offset, limit);
        if (!list)
            return NULL;
        for (size_t i = 0; i < list->len; i++) {
            int dict_index = (int)(offset + i);
            const char *key = pdf_to_name(ctx, pdf_dict_get_key(ctx, obj, dict_index));
            pdf_obj *val = pdf_dict_get_val(ctx, obj, dict_index);
            if (!fill_entry_for_obj(ctx, doc, &list->items[i], key, val)) {
                pdbg_node_list_drop(list);
                return NULL;
            }
        }
        return list;
    }

    if (pdf_is_array(ctx, obj)) {
        size_t total = (size_t)pdf_array_len(ctx, obj);
        pdbg_node_list *list = alloc_node_list(total, offset, limit);
        if (!list)
            return NULL;
        for (size_t i = 0; i < list->len; i++) {
            size_t item_index = offset + i;
            char key[32];
            snprintf(key, sizeof(key), "%zu", item_index);
            pdf_obj *val = pdf_array_get(ctx, obj, (int)item_index);
            if (!fill_entry_for_obj(ctx, doc, &list->items[i], key, val)) {
                pdbg_node_list_drop(list);
                return NULL;
            }
        }
        return list;
    }

    return alloc_node_list(0, offset, limit);
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
    doc->next_path_token = 1;
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
    if (doc->owner && doc->owner->ctx)
        registry_drop(doc->owner->ctx, doc);
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
    if (!doc || !node || !out) {
        set_error(err, PDBG_ERROR_GENERIC, "invalid children arguments");
        return PDBG_ERROR_GENERIC;
    }
    *out = NULL;
    if (!doc->authenticated) {
        set_error(err, PDBG_ERROR_PASSWORD, "document requires password before tree traversal");
        return PDBG_ERROR_PASSWORD;
    }

    fz_context *ctx = doc->owner->ctx;
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

    fz_context *ctx = doc->owner->ctx;
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
            out->preview = object_preview(ctx, obj);
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
            if (pdf_is_stream(ctx, obj)) {
                out->has_stream = 1;
                if (node->has_object)
                    out->stream.object = node->object;
                out->stream.can_decode = 1;
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
