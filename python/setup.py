from setuptools import setup, find_packages

setup(
    name="zero-ads-sdk",
    version="0.1.0",
    description="0-ads SDK: Turn idle Agents into decentralized billboards.",
    author="0-protocol",
    packages=find_packages(),
    install_requires=["requests"],
    entry_points={
        "console_scripts": [
            "zero-ads=zero_ads_sdk.cli:main"
        ]
    }
)
