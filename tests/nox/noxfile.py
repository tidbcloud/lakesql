# Copyright 2025 TiDB Cloud
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

import nox
import os


def generate_params1():
    for db_version in ["1.2.803", "1.2.791"]:
        for query_result_format in ["arrow", "json"]:
            v = tuple(map(int, db_version.split(".")))
            if query_result_format == "arrow" and v < (1, 2, 836):
                continue
            yield nox.param(db_version, query_result_format)


@nox.session
@nox.parametrize(["db_version", "query_result_format"], generate_params1())
def new_driver_with_old_servers(session, db_version, query_result_format):
    query_version = f"v{db_version}-nightly"
    session.install("behave")
    # cd bindings/python
    # maturin build --out dist
    d = "../../bindings/python/dist/"
    wheels = list(os.listdir(d))
    assert len(wheels) == 1
    for p in wheels:
        session.install(d + p)
    with session.chdir(".."):
        env = {
            "DATABEND_QUERY_VERSION": query_version,
            "DATABEND_META_VERSION": query_version,
            "DB_VERSION": db_version,
            "QUERY_RESULT_FORMAT": query_result_format,
        }
        session.run("make", "test-bindings-python", env=env)
        session.run("make", "down")


# TODO: tidbcloudlake-driver is not yet published to PyPI. Populate this
# list once released (e.g. ["0.33.7", ...]) to re-enable the
# `new_test_with_old_drivers` session below.
OLD_DRIVER_VERSIONS: list[str] = []


def generate_params2():
    for driver_version in OLD_DRIVER_VERSIONS:
        for query_result_format in ["arrow", "json"]:
            v = tuple(map(int, driver_version.split(".")))
            if query_result_format == "arrow" and v <= (0, 30, 3):
                continue
            yield nox.param(driver_version, query_result_format)


if OLD_DRIVER_VERSIONS:

    @nox.session
    @nox.parametrize(["driver_version", "query_result_format"], generate_params2())
    def new_test_with_old_drivers(session, driver_version, query_result_format):
        session.install("behave")
        session.install(f"tidbcloudlake-driver=={driver_version}")
        with session.chdir(".."):
            env = {
                "DRIVER_VERSION": driver_version,
                "QUERY_RESULT_FORMAT": query_result_format,
            }
            session.run("make", "test-bindings-python", env=env)
            session.run("make", "down")
