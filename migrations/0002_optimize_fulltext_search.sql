CREATE OR REPLACE FUNCTION make_last_term_prefix_tsquery (config regconfig, input text) RETURNS tsquery LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE
    terms text[];
    n int;
    query_text text := '';
    term text;
    i int;
BEGIN
    IF input IS NULL OR btrim(input) = '' THEN
        RETURN NULL;
    END IF;

    terms := regexp_split_to_array(trim(input), '\s+');
    n := array_length(terms, 1);

    IF n IS NULL OR n = 0 THEN
        RETURN NULL;
    END IF;

    FOR i IN 1..n LOOP
        term := regexp_replace(terms[i], '[''&|!():*<>]', '', 'g');

        IF term = '' THEN
            CONTINUE;
        END IF;

        IF query_text <> '' THEN
            query_text := query_text || ' & ';
        END IF;

        IF i = n THEN
            query_text := query_text || term || ':*';
        ELSE
            query_text := query_text || term;
        END IF;
    END LOOP;

    IF query_text = '' THEN
        RETURN NULL;
    END IF;

    RETURN to_tsquery(config, query_text);
END;
$$;
