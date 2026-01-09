export SHA256SUM := if os() == "linux" {`echo "sha256sum"`} else { `echo "shasum --algorithm 256"` }

_encrypt path:
  #!/usr/bin/env bash
  file="{{path}}"

  echo "Encrypting ${file}"
  if cat ${file}.sha256sum | $SHA256SUM --check --status && ! [ -n "$FORCE_ENCRYPTION" ]
  then
    echo " - matches existing hash, skipping"
  else
    cat $file | gzip --stdout | age --encrypt -R {{source_directory()}}/age.keys > ${file}.encrypted
    $SHA256SUM $file > ${file}.sha256sum
  fi

_decrypt path:
  #!/usr/bin/env bash
  echo "Decrypting {{path}}"
  cat {{path}}.encrypted | age --decrypt -i "${SOPS_AGE_KEY_FILE}" | gzip --decompress --stdout > {{path}}


kubernetes-secrets-encrypt:
  #!/usr/bin/env bash
  for file in $(find kubernetes -name secret.yaml -o -name "*.secret.yaml")
  do
    just _encrypt $file
  done

kubernetes-secrets-decrypt:
  #!/usr/bin/env bash
  for file in $(find . -name secret.yaml.encrypted -o -name ".secret.yaml.encrypted")
  do
    just _decrypt ${file%.encrypted}
  done
