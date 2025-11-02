#include <omp.h>

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <optional>
#include <sstream>
#include <stdexcept>
#include <string>
#include <tuple>
#include <utility>
#include <vector>
#include <sys/resource.h>

namespace {

struct FactorStep {
  uint64_t factor{};
  uint64_t remainder_before{};
  size_t modulus_tests{};
};

struct FactorRun {
  uint64_t number{};
  int threads{};
  int repeat_index{};
  double elapsed_ms{};
  size_t modulus_tests{};
  long max_rss_kb{};
  std::vector<uint64_t> factors;
  std::vector<FactorStep> steps;
};

struct Config {
  std::vector<uint64_t> numbers;
  int min_threads = 1;
  int max_threads = omp_get_max_threads();
  int repeats = 1;
  std::string schedule = "dynamic";
  int chunk_size = 0;
  std::optional<std::string> output_path;
  bool verbose = false;
};

struct ParsedRange {
  int min{};
  int max{};
};

[[noreturn]] void usage_and_exit(const char *program) {
  std::cerr << "Usage: " << program
            << " --numbers <comma-separated>"
               " [--threads <min-max>] [--repeats N]"
               " [--schedule static|dynamic|guided|auto]"
               " [--chunk SIZE] [--output FILE]"
               " [--verbose]\n";
  std::exit(EXIT_FAILURE);
}

ParsedRange parse_thread_range(const std::string &value) {
  auto dash = value.find('-');
  if (dash == std::string::npos) {
    int t = std::stoi(value);
    return {t, t};
  }
  int min = std::stoi(value.substr(0, dash));
  int max = std::stoi(value.substr(dash + 1));
  if (min <= 0 || max <= 0 || min > max) {
    throw std::invalid_argument("invalid thread range: " + value);
  }
  return {min, max};
}

std::vector<uint64_t> parse_numbers(const std::string &value) {
  std::vector<uint64_t> numbers;
  std::stringstream ss(value);
  std::string token;
  while (std::getline(ss, token, ',')) {
    if (token.empty()) {
      continue;
    }
    uint64_t n = 0;
    try {
      n = std::stoull(token);
    } catch (const std::exception &) {
      throw std::invalid_argument("invalid number: " + token);
    }
    if (n < 2) {
      throw std::invalid_argument("numbers must be >= 2");
    }
    numbers.push_back(n);
  }
  if (numbers.empty()) {
    throw std::invalid_argument("no numbers parsed from --numbers");
  }
  return numbers;
}

Config parse_args(int argc, char **argv) {
  Config config;
  for (int i = 1; i < argc; ++i) {
    std::string arg = argv[i];
    auto require_value = [&](const char *name) -> std::string {
      if (i + 1 >= argc) {
        throw std::invalid_argument(std::string("missing value for ") + name);
      }
      return argv[++i];
    };

    if (arg == "--numbers") {
      config.numbers = parse_numbers(require_value("--numbers"));
    } else if (arg == "--threads") {
      ParsedRange range = parse_thread_range(require_value("--threads"));
      config.min_threads = range.min;
      config.max_threads = range.max;
    } else if (arg == "--repeats") {
      config.repeats = std::stoi(require_value("--repeats"));
      if (config.repeats <= 0) {
        throw std::invalid_argument("--repeats must be > 0");
      }
    } else if (arg == "--schedule") {
      config.schedule = require_value("--schedule");
      if (config.schedule != "static" && config.schedule != "dynamic" &&
          config.schedule != "guided" && config.schedule != "auto") {
        throw std::invalid_argument("unsupported schedule: " + config.schedule);
      }
    } else if (arg == "--chunk") {
      config.chunk_size = std::stoi(require_value("--chunk"));
      if (config.chunk_size < 0) {
        throw std::invalid_argument("--chunk must be >= 0");
      }
    } else if (arg == "--output") {
      config.output_path = require_value("--output");
    } else if (arg == "--verbose") {
      config.verbose = true;
    } else if (arg == "--help" || arg == "-h") {
      usage_and_exit(argv[0]);
    } else {
      throw std::invalid_argument("unknown argument: " + arg);
    }
  }

  if (config.numbers.empty()) {
    usage_and_exit(argv[0]);
  }
  if (config.min_threads <= 0 || config.max_threads <= 0 ||
      config.min_threads > config.max_threads) {
    throw std::invalid_argument("invalid thread bounds");
  }
  return config;
}

omp_sched_t to_schedule_kind(const std::string &name) {
  if (name == "static") {
    return omp_sched_static;
  }
  if (name == "dynamic") {
    return omp_sched_dynamic;
  }
  if (name == "guided") {
    return omp_sched_guided;
  }
  if (name == "auto") {
    return omp_sched_auto;
  }
  return omp_sched_auto;
}

uint64_t find_smallest_factor_parallel(uint64_t n, int threads,
                                       size_t &mod_tests) {
  if (n % 2 == 0) {
    mod_tests += 1;
    return 2;
  }
  if (n % 3 == 0) {
    mod_tests += 1;
    return 3;
  }

  const uint64_t limit = static_cast<uint64_t>(std::sqrt(static_cast<long double>(n)));
  uint64_t best_factor = n;

  omp_set_num_threads(threads);
  size_t local_tests = 0;

#pragma omp parallel for schedule(runtime) reduction(min : best_factor) reduction(+: local_tests)
  for (uint64_t candidate = 5; candidate <= limit; candidate += 6) {
    const uint64_t offsets[2] = {0, 2};
    for (uint64_t offset : offsets) {
      uint64_t value = candidate + offset;
      if (value > limit) {
        continue;
      }
      local_tests += 1;
      if (value >= best_factor) {
        continue;
      }
      if (n % value == 0) {
        best_factor = value;
      }
    }
  }

  mod_tests += local_tests;
  return best_factor == n ? n : best_factor;
}

long read_max_rss_kb() {
#ifdef __linux__
  struct rusage usage {};
  if (getrusage(RUSAGE_SELF, &usage) == 0) {
    return usage.ru_maxrss;
  }
#endif
  return -1;
}

FactorRun factor_number(uint64_t number, int threads) {
  FactorRun run{};
  run.number = number;
  run.threads = threads;

  size_t modulus_tests = 0;
  std::vector<uint64_t> factors;
  std::vector<FactorStep> steps;

  auto start = std::chrono::steady_clock::now();
  uint64_t remainder = number;

  while (remainder > 1) {
    FactorStep step{};
    step.remainder_before = remainder;
    size_t tests_before = modulus_tests;
    uint64_t factor =
        find_smallest_factor_parallel(remainder, threads, modulus_tests);
    step.factor = factor;
    step.modulus_tests = modulus_tests - tests_before;
    factors.push_back(factor);
    steps.push_back(step);
    remainder /= factor;
  }

  auto end = std::chrono::steady_clock::now();
  run.elapsed_ms =
      std::chrono::duration_cast<std::chrono::duration<double, std::milli>>(end - start)
          .count();
  run.modulus_tests = modulus_tests;
  run.factors = std::move(factors);
  run.steps = std::move(steps);
  run.max_rss_kb = read_max_rss_kb();
  return run;
}

void maybe_write_csv_header(std::ostream &out) {
  out << "number,threads,repeat,time_ms,modulus_tests,max_rss_kb,factors\n";
}

void write_run_csv(const FactorRun &run, std::ostream &out) {
  out << run.number << ',' << run.threads << ',' << run.repeat_index << ','
      << run.elapsed_ms << ',' << run.modulus_tests << ',' << run.max_rss_kb
      << ',';
  for (size_t i = 0; i < run.factors.size(); ++i) {
    out << run.factors[i];
    if (i + 1 < run.factors.size()) {
      out << 'x';
    }
  }
  out << '\n';
}

void print_run_summary(const FactorRun &run) {
  std::cout << "n=" << run.number << " threads=" << run.threads
            << " repeat=" << run.repeat_index << " time(ms)=" << run.elapsed_ms
            << " modulus_tests=" << run.modulus_tests;
  if (run.max_rss_kb >= 0) {
    std::cout << " max_rss(kb)=" << run.max_rss_kb;
  }
  std::cout << " factors=";
  for (size_t i = 0; i < run.factors.size(); ++i) {
    std::cout << run.factors[i];
    if (i + 1 < run.factors.size()) {
      std::cout << '*';
    }
  }
  std::cout << '\n';
}

void prepare_schedule(const Config &config) {
  omp_sched_t kind = to_schedule_kind(config.schedule);
  int chunk = config.chunk_size;
  if (chunk <= 0) {
    chunk = 1;
  }
  omp_set_schedule(kind, chunk);
  omp_set_dynamic(0);
}

}  // namespace

int main(int argc, char **argv) {
  Config config{};
  try {
    config = parse_args(argc, argv);
  } catch (const std::exception &ex) {
    std::cerr << "Error: " << ex.what() << "\n";
    usage_and_exit(argv[0]);
  }

  prepare_schedule(config);

  std::ofstream csv_file;
  std::ostream *csv_stream = nullptr;
  if (config.output_path) {
    csv_file.open(*config.output_path, std::ios::out | std::ios::trunc);
    if (!csv_file) {
      std::cerr << "Failed to open output file: " << *config.output_path << "\n";
      return EXIT_FAILURE;
    }
    csv_stream = &csv_file;
    maybe_write_csv_header(*csv_stream);
  }

  if (config.verbose && !config.output_path) {
    maybe_write_csv_header(std::cout);
  }

  for (uint64_t number : config.numbers) {
    for (int threads = config.min_threads; threads <= config.max_threads; ++threads) {
      for (int repeat = 1; repeat <= config.repeats; ++repeat) {
        FactorRun run = factor_number(number, threads);
        run.repeat_index = repeat;

        if (config.verbose && !config.output_path) {
          write_run_csv(run, std::cout);
        } else {
          print_run_summary(run);
        }

        if (csv_stream) {
          write_run_csv(run, *csv_stream);
        }
      }
    }
  }

  return EXIT_SUCCESS;
}
