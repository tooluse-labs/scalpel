void good(void *ctx)
{
    int status = 0;
    fz_try(ctx)
    {
        status = 1;
        break;
    }
    fz_catch(ctx)
    {
        status = 2;
    }
    (void)status;
}

