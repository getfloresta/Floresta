"""
tests/test_framework/crypto/pkcs8.py

This module generate proper PKCS#8 private keys and certificate
"""

import os
from datetime import datetime, timedelta, timezone
from typing import Tuple

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.hazmat.primitives.asymmetric.rsa import RSAPrivateKey
from cryptography.x509.oid import NameOID

DEFAULT_PUBLIC_EXPONENT = 65537
DEFAULT_KEY_SIZE = 2048
DEFAULT_CN = "getfloresta"
DEFAULT_DAYS = 1


def create_pkcs8_private_key(
    path: str,
    public_exponent: int = DEFAULT_PUBLIC_EXPONENT,
    key_size: int = DEFAULT_KEY_SIZE,
) -> Tuple[str, RSAPrivateKey]:
    """
    Generate private key in a proper format PKCS#8
    """
    pk = rsa.generate_private_key(public_exponent=public_exponent, key_size=key_size)

    # Serialize and save key
    pem = pk.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption(),
    )
    pk_path = os.path.join(path, "key.pem")
    with open(pk_path, "wb") as f:
        f.write(pem)

    return (pk_path, pk)


def create_pkcs8_self_signed_certificate(
    path: str,
    pk: RSAPrivateKey,
    common_name: str = DEFAULT_CN,
    validity_days: int = DEFAULT_DAYS,
) -> str:
    """
    Generate a self signed certificate  in a proper format PKCS#8
    """
    # Create subject/issuer name
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, common_name)])

    # Certificate validity period
    now = datetime.now(timezone.utc)
    validity = timedelta(days=validity_days)

    # Build and sign certificate
    cert = (
        x509.CertificateBuilder()
        .subject_name(subject)
        .issuer_name(subject)
        .public_key(pk.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now)
        .not_valid_after(now + validity)
        .sign(pk, hashes.SHA256())
    )

    # Save certificate
    cert_path = os.path.join(path, "cert.pem")
    with open(cert_path, "wb") as f:
        f.write(cert.public_bytes(serialization.Encoding.PEM))

    return cert_path
