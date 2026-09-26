// A C entry point into lld (LLVM 18's linker, linked into the fermium binary): `fermium build` links the
// program's object file with it, so building an executable needs no system linker (spec B5.10).
//
// The driver is called directly (not through lld::lldMain, which initializes every LLVM target for LTO and
// would pull all of them into the binary).
#include "lld/Common/CommonLinkerContext.h"
#include "lld/Common/Driver.h"
#include "llvm/Support/raw_ostream.h"

#include <cstring>
#include <string>
#include <vector>

#ifdef FERMIUM_LLD_MACHO
LLD_HAS_DRIVER(macho)
#else
LLD_HAS_DRIVER(elf)
#endif

// argv[0] is the linker's name ("ld.lld" / "ld64.lld"). The linker's messages go to `msg` (at most cap bytes,
// NUL-terminated). Returns 0 when the executable was linked.
extern "C" int fermium_lld_link(int argc, const char **argv, char *msg, size_t cap) {
  std::vector<const char *> args(argv, argv + argc);
  std::string out, err;
  llvm::raw_string_ostream os(out), es(err);
#ifdef FERMIUM_LLD_MACHO
  bool ok = lld::macho::link(args, os, es, /*exitEarly=*/false, /*disableOutput=*/false);
#else
  bool ok = lld::elf::link(args, os, es, /*exitEarly=*/false, /*disableOutput=*/false);
#endif
  lld::CommonLinkerContext::destroy();
  os.flush();
  es.flush();
  std::string all = err + out;
  if (cap > 0) {
    size_t n = all.size() < cap - 1 ? all.size() : cap - 1;
    std::memcpy(msg, all.data(), n);
    msg[n] = 0;
  }
  return ok ? 0 : 1;
}
