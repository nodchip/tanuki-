#include <algorithm>
#include <array>
#include <chrono>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <limits>
#include <sstream>
#include <stdexcept>
#include <string>
#include <unordered_set>
#include <unordered_map>
#include <vector>

namespace {

constexpr int BLACK = 0;
constexpr int WHITE = 1;
constexpr int KING = 8;
constexpr int PIECE_PROMOTE = 8;
constexpr int PIECE_WHITE = 16;
constexpr std::uint16_t MOVE_DROP = 1u << 14;
constexpr std::uint16_t MOVE_PROMOTE = 1u << 15;

struct HuffmanCode {
	int code;
	int bits;
};

constexpr std::array<HuffmanCode, 8> HUFFMAN_TABLE = {{
	{0x00, 1},
	{0x01, 2},
	{0x03, 4},
	{0x0b, 4},
	{0x07, 4},
	{0x1f, 6},
	{0x3f, 6},
	{0x0f, 5},
}};

struct BookMove {
	std::string move;
	std::string next_move;
	int value = 0;
};

struct Stats {
	std::uint64_t positions = 0;
	std::uint64_t moves = 0;
	std::uint64_t records = 0;
	std::uint64_t skipped_positions = 0;
};

struct UpdateStats {
	std::uint64_t book_positions = 0;
	std::uint64_t book_moves = 0;
	std::uint64_t root_keys = 0;
	std::uint64_t after_move_keys = 0;
	std::uint64_t samples = 0;
	std::uint64_t overwritten = 0;
	std::uint64_t used_book_keys = 0;
};

struct Options {
	std::filesystem::path input;
	std::filesystem::path output;
	double progress_interval_sec = 5.0;
};

struct UpdateOptions {
	std::filesystem::path input_training;
	std::filesystem::path book;
	std::filesystem::path output_training;
	double progress_interval_sec = 5.0;
};

struct BitWriter {
	std::array<std::uint8_t, 32> data{};
	int cursor = 0;

	void write_one_bit(int value) {
		if (value != 0) {
			data[static_cast<std::size_t>(cursor / 8)] |= static_cast<std::uint8_t>(1u << (cursor & 7));
		}
		++cursor;
	}

	void write_bits(int value, int bit_count) {
		for (int i = 0; i < bit_count; ++i) {
			write_one_bit(value & (1 << i));
		}
	}
};

std::string trim(const std::string& input) {
	const auto first = input.find_first_not_of(" \t\r\n");
	if (first == std::string::npos) {
		return "";
	}
	const auto last = input.find_last_not_of(" \t\r\n");
	return input.substr(first, last - first + 1);
}

bool starts_with(const std::string& text, const char* prefix) {
	const std::string p(prefix);
	return text.size() >= p.size() && text.compare(0, p.size(), p) == 0;
}

int piece_type(char c) {
	switch (c) {
	case 'P': case 'p': return 1;
	case 'L': case 'l': return 2;
	case 'N': case 'n': return 3;
	case 'S': case 's': return 4;
	case 'B': case 'b': return 5;
	case 'R': case 'r': return 6;
	case 'G': case 'g': return 7;
	case 'K': case 'k': return 8;
	default:
		throw std::runtime_error(std::string("invalid piece: ") + c);
	}
}

int color_of_char(char c) {
	return ('A' <= c && c <= 'Z') ? BLACK : WHITE;
}

int square_from_file_rank(int file, int rank) {
	return (file - 1) * 9 + (rank - 1);
}

int parse_square(const std::string& text, std::size_t offset) {
	if (offset + 1 >= text.size()) {
		throw std::runtime_error("invalid move square: " + text);
	}
	const int file = text[offset] - '0';
	const int rank = text[offset + 1] - 'a' + 1;
	if (file < 1 || file > 9 || rank < 1 || rank > 9) {
		throw std::runtime_error("invalid move square: " + text);
	}
	return square_from_file_rank(file, rank);
}

int make_piece(int color, int pt, bool promoted) {
	return (color << 4) + pt + (promoted ? PIECE_PROMOTE : 0);
}

int raw_type_of(int piece) {
	return piece & 7;
}

int color_of_piece(int piece) {
	return (piece & PIECE_WHITE) >> 4;
}

std::vector<std::string> split_tokens(const std::string& text) {
	std::istringstream iss(text);
	std::vector<std::string> tokens;
	std::string token;
	while (iss >> token) {
		tokens.push_back(token);
	}
	return tokens;
}

std::uint16_t parse_ply(const std::string& sfen) {
	const auto tokens = split_tokens(sfen);
	if (tokens.size() < 4) {
		return 1;
	}
	const auto value = std::stoul(tokens[3]);
	return static_cast<std::uint16_t>(std::min<unsigned long>(value, std::numeric_limits<std::uint16_t>::max()));
}

std::uint16_t move16_from_usi(const std::string& move) {
	if (move.size() < 4) {
		throw std::runtime_error("invalid move: " + move);
	}
	const int to = parse_square(move, 2);
	if (move[1] == '*') {
		return static_cast<std::uint16_t>(to + (piece_type(move[0]) << 7) + MOVE_DROP);
	}
	const int from = parse_square(move, 0);
	std::uint16_t result = static_cast<std::uint16_t>(to + (from << 7));
	if (move.size() == 5 && move[4] == '+') {
		result = static_cast<std::uint16_t>(result + MOVE_PROMOTE);
	}
	return result;
}

BookMove parse_book_move(const std::string& line, bool& ok) {
	ok = false;
	std::istringstream iss(line);
	std::string move;
	std::string next_move;
	int value = 0;
	if (!(iss >> move >> next_move >> value)) {
		return {};
	}
	if (move == "none" || move == "resign") {
		return {};
	}
	ok = true;
	return BookMove{move, next_move, value};
}

struct ParsedSfen {
	std::array<int, 81> board{};
	std::unordered_map<int, int> hand_counts;
	int side_to_move = BLACK;
	std::uint16_t ply = 1;
};

int hand_key(int color, int pt) {
	return color * 16 + pt;
}

ParsedSfen parse_sfen(const std::string& sfen) {
	const auto tokens = split_tokens(sfen);
	if (tokens.size() < 4) {
		throw std::runtime_error("invalid SFEN: " + sfen);
	}

	ParsedSfen parsed;
	parsed.board.fill(0);
	parsed.side_to_move = tokens[1] == "b" ? BLACK : WHITE;
	parsed.ply = parse_ply(sfen);

	int rank = 1;
	int file = 9;
	bool promoted = false;
	for (char c : tokens[0]) {
		if (c == '/') {
			if (file != 0) {
				throw std::runtime_error("invalid SFEN rank: " + sfen);
			}
			++rank;
			file = 9;
			continue;
		}
		if (c == '+') {
			promoted = true;
			continue;
		}
		if ('1' <= c && c <= '9') {
			file -= c - '0';
			continue;
		}
		if (rank < 1 || rank > 9 || file < 1 || file > 9) {
			throw std::runtime_error("invalid SFEN board: " + sfen);
		}
		parsed.board[static_cast<std::size_t>(square_from_file_rank(file, rank))] =
			make_piece(color_of_char(c), piece_type(c), promoted);
		promoted = false;
		--file;
	}
	if (rank != 9 || file != 0) {
		throw std::runtime_error("invalid SFEN board: " + sfen);
	}

	if (tokens[2] != "-") {
		int count = 0;
		for (char c : tokens[2]) {
			if ('0' <= c && c <= '9') {
				count = count * 10 + (c - '0');
				continue;
			}
			const int n = count == 0 ? 1 : count;
			parsed.hand_counts[hand_key(color_of_char(c), piece_type(c))] += n;
			count = 0;
		}
	}

	return parsed;
}

int piece_type_of(int piece) {
	return piece & 15;
}

std::string format_board(const std::array<int, 81>& board) {
	const std::array<std::string, 15> piece_to_text = {{
		"", "P", "L", "N", "S", "B", "R", "G", "K", "+P", "+L", "+N", "+S", "+B", "+R"
	}};

	std::string result;
	for (int rank = 1; rank <= 9; ++rank) {
		if (rank != 1) {
			result += '/';
		}
		int empty = 0;
		for (int file = 9; file >= 1; --file) {
			const int piece = board[static_cast<std::size_t>(square_from_file_rank(file, rank))];
			if (piece == 0) {
				++empty;
				continue;
			}
			if (empty != 0) {
				result += static_cast<char>('0' + empty);
				empty = 0;
			}
			auto text = piece_to_text[static_cast<std::size_t>(piece_type_of(piece))];
			if (color_of_piece(piece) == WHITE) {
				for (char& c : text) {
					if ('A' <= c && c <= 'Z') {
						c = static_cast<char>(c - 'A' + 'a');
					}
				}
			}
			result += text;
		}
		if (empty != 0) {
			result += static_cast<char>('0' + empty);
		}
	}
	return result;
}

std::string format_hands(const std::unordered_map<int, int>& hand_counts) {
	std::string result;
	const std::array<std::pair<int, char>, 14> order = {{
		{hand_key(BLACK, 6), 'R'}, {hand_key(BLACK, 5), 'B'}, {hand_key(BLACK, 7), 'G'},
		{hand_key(BLACK, 4), 'S'}, {hand_key(BLACK, 2), 'L'}, {hand_key(BLACK, 3), 'N'},
		{hand_key(BLACK, 1), 'P'}, {hand_key(WHITE, 6), 'r'}, {hand_key(WHITE, 5), 'b'},
		{hand_key(WHITE, 7), 'g'}, {hand_key(WHITE, 4), 's'}, {hand_key(WHITE, 2), 'l'},
		{hand_key(WHITE, 3), 'n'}, {hand_key(WHITE, 1), 'p'},
	}};
	for (const auto& item : order) {
		const auto it = hand_counts.find(item.first);
		if (it == hand_counts.end() || it->second <= 0) {
			continue;
		}
		if (it->second > 1) {
			result += std::to_string(it->second);
		}
		result += item.second;
	}
	return result.empty() ? "-" : result;
}

std::string format_sfen(const ParsedSfen& parsed) {
	return format_board(parsed.board)
		+ (parsed.side_to_move == BLACK ? " b " : " w ")
		+ format_hands(parsed.hand_counts)
		+ " "
		+ std::to_string(parsed.ply);
}

std::string apply_move_to_sfen(const std::string& sfen, const std::string& move) {
	auto parsed = parse_sfen(sfen);
	const int to = parse_square(move, 2);

	if (move.size() >= 2 && move[1] == '*') {
		const int pt = piece_type(move[0]);
		const int key = hand_key(parsed.side_to_move, pt);
		auto it = parsed.hand_counts.find(key);
		if (it == parsed.hand_counts.end() || it->second <= 0) {
			throw std::runtime_error("missing hand piece for move: " + move + " sfen: " + sfen);
		}
		if (--it->second == 0) {
			parsed.hand_counts.erase(it);
		}
		parsed.board[static_cast<std::size_t>(to)] = make_piece(parsed.side_to_move, pt, false);
	} else {
		const int from = parse_square(move, 0);
		int piece = parsed.board[static_cast<std::size_t>(from)];
		if (piece == 0) {
			throw std::runtime_error("missing board piece for move: " + move + " sfen: " + sfen);
		}
		const int captured = parsed.board[static_cast<std::size_t>(to)];
		if (captured != 0) {
			parsed.hand_counts[hand_key(parsed.side_to_move, raw_type_of(captured))]++;
		}
		if (move.size() == 5 && move[4] == '+') {
			piece += PIECE_PROMOTE;
		}
		parsed.board[static_cast<std::size_t>(from)] = 0;
		parsed.board[static_cast<std::size_t>(to)] = piece;
	}

	parsed.side_to_move = parsed.side_to_move == BLACK ? WHITE : BLACK;
	if (parsed.ply != std::numeric_limits<std::uint16_t>::max()) {
		++parsed.ply;
	}
	return format_sfen(parsed);
}

void write_board_piece(BitWriter& writer, int piece) {
	const int pt = raw_type_of(piece);
	const auto code = HUFFMAN_TABLE[static_cast<std::size_t>(pt)];
	writer.write_bits(code.code, code.bits);
	if (piece == 0) {
		return;
	}
	if (pt != 7) {
		writer.write_one_bit(piece & PIECE_PROMOTE);
	}
	writer.write_one_bit(color_of_piece(piece));
}

void write_hand_piece(BitWriter& writer, int color, int pt) {
	const auto code = HUFFMAN_TABLE[static_cast<std::size_t>(pt)];
	writer.write_bits(code.code >> 1, code.bits - 1);
	if (pt != 7) {
		writer.write_one_bit(0);
	}
	writer.write_one_bit(color);
}

std::array<std::uint8_t, 32> pack_sfen(const std::string& sfen) {
	const auto parsed = parse_sfen(sfen);
	BitWriter writer;
	writer.write_one_bit(parsed.side_to_move);

	for (int color : {BLACK, WHITE}) {
		const int king = make_piece(color, KING, false);
		const auto it = std::find(parsed.board.begin(), parsed.board.end(), king);
		if (it == parsed.board.end()) {
			throw std::runtime_error("king not found in SFEN: " + sfen);
		}
		writer.write_bits(static_cast<int>(std::distance(parsed.board.begin(), it)), 7);
	}

	for (int piece : parsed.board) {
		if ((piece & 15) == KING) {
			continue;
		}
		write_board_piece(writer, piece);
	}

	for (int color : {BLACK, WHITE}) {
		for (int pt = 1; pt < KING; ++pt) {
			const auto it = parsed.hand_counts.find(hand_key(color, pt));
			const int count = it == parsed.hand_counts.end() ? 0 : it->second;
			for (int i = 0; i < count; ++i) {
				write_hand_piece(writer, color, pt);
			}
		}
	}

	if (writer.cursor != 256) {
		throw std::runtime_error("packed SFEN is not 256 bits: " + sfen);
	}
	return writer.data;
}

std::string packed_key_from_sfen(const std::string& sfen) {
	const auto packed = pack_sfen(sfen);
	return std::string(reinterpret_cast<const char*>(packed.data()), packed.size());
}

void upsert_max(std::unordered_map<std::string, int>& values, const std::string& key, int value) {
	const auto it = values.find(key);
	if (it == values.end() || value > it->second) {
		values[key] = value;
	}
}

void write_u16(std::ofstream& out, std::uint16_t value) {
	const char bytes[2] = {
		static_cast<char>(value & 0xff),
		static_cast<char>((value >> 8) & 0xff),
	};
	out.write(bytes, 2);
}

void write_s16(std::ofstream& out, int value) {
	const int clamped = std::max(-32768, std::min(32767, value));
	write_u16(out, static_cast<std::uint16_t>(static_cast<std::int16_t>(clamped)));
}

void write_record(std::ofstream& out, const std::string& sfen, const BookMove& move) {
	const auto packed = pack_sfen(sfen);
	out.write(reinterpret_cast<const char*>(packed.data()), static_cast<std::streamsize>(packed.size()));
	write_s16(out, move.value);
	write_u16(out, (move.next_move == "none" || move.next_move == "resign") ? 0 : move16_from_usi(move.next_move));
	write_u16(out, parse_ply(sfen));
	const char tail[2] = {0, 0};
	out.write(tail, 2);
}

class ProgressReporter {
public:
	ProgressReporter(std::uintmax_t total_bytes, double interval_sec)
		: total_bytes_(total_bytes),
		  interval_(std::chrono::duration<double>(interval_sec)),
		  start_(std::chrono::steady_clock::now()),
		  last_(start_) {}

	void maybe_report(std::uintmax_t processed_bytes, const Stats& stats, bool force = false) {
		const auto now = std::chrono::steady_clock::now();
		if (!force && now - last_ < interval_) {
			return;
		}
		const double elapsed = std::chrono::duration<double>(now - start_).count();
		const double percent = total_bytes_ == 0 ? 100.0 : 100.0 * static_cast<double>(processed_bytes) / static_cast<double>(total_bytes_);
		const double mib = static_cast<double>(processed_bytes) / (1024.0 * 1024.0);
		const double mib_per_sec = elapsed > 0.0 ? mib / elapsed : 0.0;
		const double eta = (processed_bytes > 0 && mib_per_sec > 0.0)
			? (static_cast<double>(total_bytes_ - processed_bytes) / (1024.0 * 1024.0)) / mib_per_sec
			: 0.0;

		std::cerr
			<< "progress"
			<< " positions=" << stats.positions
			<< " records=" << stats.records
			<< " moves=" << stats.moves
			<< " skipped=" << stats.skipped_positions
			<< " bytes=" << processed_bytes << "/" << total_bytes_
			<< " percent=" << std::fixed << std::setprecision(2) << percent
			<< " MiB_per_sec=" << std::setprecision(2) << mib_per_sec
			<< " eta_sec=" << std::setprecision(0) << eta
			<< std::defaultfloat
			<< '\n';
		last_ = now;
	}

private:
	std::uintmax_t total_bytes_;
	std::chrono::duration<double> interval_;
	std::chrono::steady_clock::time_point start_;
	std::chrono::steady_clock::time_point last_;
};

class UpdateProgressReporter {
public:
	UpdateProgressReporter(std::uintmax_t total_bytes, double interval_sec, std::string phase)
		: total_bytes_(total_bytes),
		  interval_(std::chrono::duration<double>(interval_sec)),
		  phase_(std::move(phase)),
		  start_(std::chrono::steady_clock::now()),
		  last_(start_) {}

	void maybe_report(std::uintmax_t processed_bytes, const UpdateStats& stats, bool force = false) {
		const auto now = std::chrono::steady_clock::now();
		if (!force && now - last_ < interval_) {
			return;
		}
		const double elapsed = std::chrono::duration<double>(now - start_).count();
		const double percent = total_bytes_ == 0 ? 100.0 : 100.0 * static_cast<double>(processed_bytes) / static_cast<double>(total_bytes_);
		const double mib = static_cast<double>(processed_bytes) / (1024.0 * 1024.0);
		const double mib_per_sec = elapsed > 0.0 ? mib / elapsed : 0.0;
		std::cerr
			<< "progress phase=" << phase_
			<< " book_positions=" << stats.book_positions
			<< " book_moves=" << stats.book_moves
			<< " root_keys=" << stats.root_keys
			<< " after_move_keys=" << stats.after_move_keys
			<< " used_book_keys=" << stats.used_book_keys
			<< " samples=" << stats.samples
			<< " overwritten=" << stats.overwritten
			<< " bytes=" << processed_bytes << "/" << total_bytes_
			<< " percent=" << std::fixed << std::setprecision(2) << percent
			<< " MiB_per_sec=" << std::setprecision(2) << mib_per_sec
			<< std::defaultfloat
			<< '\n';
		last_ = now;
	}

private:
	std::uintmax_t total_bytes_;
	std::chrono::duration<double> interval_;
	std::string phase_;
	std::chrono::steady_clock::time_point start_;
	std::chrono::steady_clock::time_point last_;
};

std::uintmax_t tell_or_total(std::ifstream& input, std::uintmax_t total_bytes) {
	const auto pos = input.tellg();
	if (pos < 0) {
		return total_bytes;
	}
	return static_cast<std::uintmax_t>(pos);
}

Stats convert(const Options& options) {
	std::ifstream input(options.input, std::ios::binary);
	if (!input) {
		throw std::runtime_error("failed to open input: " + options.input.string());
	}
	std::ofstream output(options.output, std::ios::binary | std::ios::trunc);
	if (!output) {
		throw std::runtime_error("failed to open output: " + options.output.string());
	}

	const auto total_bytes = std::filesystem::file_size(options.input);
	ProgressReporter progress(total_bytes, options.progress_interval_sec);
	Stats stats;
	std::string current_sfen;
	bool has_sfen = false;
	bool has_move_for_current_position = false;
	std::string raw_line;

	auto flush = [&]() {
		if (!has_sfen) {
			return;
		}
		++stats.positions;
		if (!has_move_for_current_position) {
			++stats.skipped_positions;
		}
		progress.maybe_report(tell_or_total(input, total_bytes), stats);
	};

	while (std::getline(input, raw_line)) {
		const std::string line = trim(raw_line);
		if (line.empty() || starts_with(line, "#") || starts_with(line, "//")) {
			continue;
		}

		if (starts_with(line, "sfen ")) {
			flush();
			current_sfen = line.substr(5);
			has_sfen = true;
			has_move_for_current_position = false;
			continue;
		}

		if (!has_sfen) {
			continue;
		}

		bool ok = false;
		const BookMove move = parse_book_move(line, ok);
		if (!ok) {
			continue;
		}
		++stats.moves;
		const std::string after_sfen = apply_move_to_sfen(current_sfen, move.move);
		write_record(output, after_sfen, BookMove{move.move, move.next_move, -move.value});
		++stats.records;
		has_move_for_current_position = true;
	}
	flush();
	progress.maybe_report(total_bytes, stats, true);
	return stats;
}

struct BookValueMaps {
	std::unordered_map<std::string, int> root_values;
	std::unordered_map<std::string, int> after_move_values;
	UpdateStats stats;
};

BookValueMaps build_book_value_maps(const std::filesystem::path& book_path, double progress_interval_sec) {
	std::ifstream input(book_path, std::ios::binary);
	if (!input) {
		throw std::runtime_error("failed to open book: " + book_path.string());
	}

	const auto total_bytes = std::filesystem::file_size(book_path);
	UpdateProgressReporter progress(total_bytes, progress_interval_sec, "book");
	BookValueMaps maps;
	std::string current_sfen;
	std::vector<BookMove> moves;
	bool has_sfen = false;
	std::string raw_line;

	auto flush = [&]() {
		if (!has_sfen) {
			return;
		}
		++maps.stats.book_positions;
		if (!moves.empty()) {
			int best_value = moves.front().value;
			for (const auto& move : moves) {
				best_value = std::max(best_value, move.value);
			}
			upsert_max(maps.root_values, packed_key_from_sfen(current_sfen), best_value);
			for (const auto& move : moves) {
				const std::string after_sfen = apply_move_to_sfen(current_sfen, move.move);
				upsert_max(maps.after_move_values, packed_key_from_sfen(after_sfen), -move.value);
			}
		}
		progress.maybe_report(tell_or_total(input, total_bytes), maps.stats);
	};

	while (std::getline(input, raw_line)) {
		const std::string line = trim(raw_line);
		if (line.empty() || starts_with(line, "#") || starts_with(line, "//")) {
			continue;
		}
		if (starts_with(line, "sfen ")) {
			flush();
			current_sfen = line.substr(5);
			moves.clear();
			has_sfen = true;
			continue;
		}
		if (!has_sfen) {
			continue;
		}
		bool ok = false;
		const BookMove move = parse_book_move(line, ok);
		if (!ok) {
			continue;
		}
		++maps.stats.book_moves;
		moves.push_back(move);
	}
	flush();
	maps.stats.root_keys = maps.root_values.size();
	maps.stats.after_move_keys = maps.after_move_values.size();
	progress.maybe_report(total_bytes, maps.stats, true);
	return maps;
}

std::int16_t clamp_s16(int value) {
	return static_cast<std::int16_t>(std::max(-32768, std::min(32767, value)));
}

void store_s16(std::array<char, 40>& record, int value) {
	const auto v = static_cast<std::uint16_t>(clamp_s16(value));
	record[32] = static_cast<char>(v & 0xff);
	record[33] = static_cast<char>((v >> 8) & 0xff);
}

double percentage(std::uint64_t numerator, std::uint64_t denominator) {
	return denominator == 0 ? 0.0 : 100.0 * static_cast<double>(numerator) / static_cast<double>(denominator);
}

UpdateStats overwrite_training_scores(const UpdateOptions& options) {
	BookValueMaps maps = build_book_value_maps(options.book, options.progress_interval_sec);
	std::ifstream input(options.input_training, std::ios::binary);
	if (!input) {
		throw std::runtime_error("failed to open input training: " + options.input_training.string());
	}
	std::ofstream output(options.output_training, std::ios::binary | std::ios::trunc);
	if (!output) {
		throw std::runtime_error("failed to open output training: " + options.output_training.string());
	}

	const auto total_bytes = std::filesystem::file_size(options.input_training);
	if (total_bytes % 40 != 0) {
		throw std::runtime_error("input training size is not a multiple of PackedSfenValue size 40");
	}
	UpdateProgressReporter progress(total_bytes, options.progress_interval_sec, "training");
	UpdateStats stats = maps.stats;
	std::unordered_set<std::string> used_book_keys;
	std::array<char, 40> record{};

	while (input.read(record.data(), static_cast<std::streamsize>(record.size()))) {
		++stats.samples;
		const std::string key(record.data(), 32);
		auto root_it = maps.root_values.find(key);
		if (root_it != maps.root_values.end()) {
			store_s16(record, root_it->second);
			++stats.overwritten;
			used_book_keys.insert(key);
		} else {
			auto after_it = maps.after_move_values.find(key);
			if (after_it != maps.after_move_values.end()) {
				store_s16(record, after_it->second);
				++stats.overwritten;
				used_book_keys.insert(key);
			}
		}
		output.write(record.data(), static_cast<std::streamsize>(record.size()));
		progress.maybe_report(tell_or_total(input, total_bytes), stats);
	}
	if (!input.eof()) {
		throw std::runtime_error("failed while reading input training");
	}
	stats.used_book_keys = used_book_keys.size();
	progress.maybe_report(total_bytes, stats, true);
	return stats;
}

Options parse_options(int argc, char** argv) {
	if (argc < 3) {
		throw std::runtime_error(
			"usage: convert_yaneuraou_book_to_packed_sfen <input.db> <output.bin> [--progress-interval seconds]");
	}

	Options options;
	options.input = argv[1];
	options.output = argv[2];
	for (int i = 3; i < argc; ++i) {
		const std::string arg = argv[i];
		if (arg == "--progress-interval" && i + 1 < argc) {
			options.progress_interval_sec = std::stod(argv[++i]);
		} else {
			throw std::runtime_error("unknown option: " + arg);
		}
	}
	return options;
}

UpdateOptions parse_update_options(int argc, char** argv) {
	if (argc < 5) {
		throw std::runtime_error(
			"usage: convert_yaneuraou_book_to_packed_sfen overwrite-scores <input_training.bin> <book.db> <output_training.bin> [--progress-interval seconds]");
	}
	UpdateOptions options;
	options.input_training = argv[2];
	options.book = argv[3];
	options.output_training = argv[4];
	for (int i = 5; i < argc; ++i) {
		const std::string arg = argv[i];
		if (arg == "--progress-interval" && i + 1 < argc) {
			options.progress_interval_sec = std::stod(argv[++i]);
		} else {
			throw std::runtime_error("unknown option: " + arg);
		}
	}
	return options;
}

} // namespace

int main(int argc, char** argv) {
	try {
		if (argc >= 2 && std::string(argv[1]) == "overwrite-scores") {
			const auto options = parse_update_options(argc, argv);
			const auto stats = overwrite_training_scores(options);
			std::cout
				<< "book_positions=" << stats.book_positions
				<< " book_moves=" << stats.book_moves
				<< " root_keys=" << stats.root_keys
				<< " after_move_keys=" << stats.after_move_keys
				<< " book_keys=" << (stats.root_keys + stats.after_move_keys)
				<< " used_book_keys=" << stats.used_book_keys
				<< " used_book_percent=" << std::fixed << std::setprecision(2)
				<< percentage(stats.used_book_keys, stats.root_keys + stats.after_move_keys)
				<< std::defaultfloat
				<< " samples=" << stats.samples
				<< " overwritten=" << stats.overwritten
				<< " overwritten_percent=" << std::fixed << std::setprecision(2)
				<< percentage(stats.overwritten, stats.samples)
				<< std::defaultfloat
				<< '\n';
			return 0;
		}
		const auto options = parse_options(argc, argv);
		const auto stats = convert(options);
		std::cout
			<< "positions=" << stats.positions
			<< " moves=" << stats.moves
			<< " records=" << stats.records
			<< " skipped_positions=" << stats.skipped_positions
			<< '\n';
		return 0;
	} catch (const std::exception& e) {
		std::cerr << "error: " << e.what() << '\n';
		return 1;
	}
}
